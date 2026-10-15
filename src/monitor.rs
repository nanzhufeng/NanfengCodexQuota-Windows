use crate::{limits::RateLimits, weekly_quota_render::WeeklyQuotaRenderModel};
use chrono::{DateTime, Local, Utc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    NotInstalled,
    NotAuthenticated,
    Network,
    Protocol,
    Timeout,
}
impl Failure {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotInstalled => "未找到 Codex",
            Self::NotAuthenticated => "请登录 Codex",
            Self::Network => "连接失败",
            Self::Protocol => "接口不兼容",
            Self::Timeout => "查询超时",
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Monitor {
    pub limits: Option<RateLimits>,
    pub refreshing: bool,
    pub failure: Option<Failure>,
    pub source: Option<String>,
}
impl Monitor {
    pub fn started(&mut self) {
        self.refreshing = true;
    }
    pub fn succeeded(&mut self, limits: RateLimits, source: String) {
        self.limits = Some(limits);
        self.source = Some(source);
        self.refreshing = false;
        self.failure = None;
    }
    pub fn failed(&mut self, failure: Failure) {
        self.failure = Some(failure);
        self.refreshing = false;
    }
    pub fn stale(&self, now: DateTime<Utc>, interval: u64) -> bool {
        self.limits
            .as_ref()
            .is_some_and(|l| (now - l.sampled_at).num_seconds() > (interval * 2).max(120) as i64)
    }
    pub fn actionable_failure(&self) -> Option<Failure> {
        self.failure
            .filter(|f| !matches!(f, Failure::Network | Failure::Timeout))
    }
    pub fn status_label(&self, now: DateTime<Utc>, interval: u64) -> &'static str {
        if let Some(f) = self.actionable_failure() {
            f.label()
        } else if self.stale(now, interval) {
            "数据过期"
        } else if self.limits.is_some() && self.failure.is_some() {
            "最近更新"
        } else if self.limits.is_some() {
            "已连接"
        } else {
            "等待查询"
        }
    }
    pub fn render_model(&self, now: DateTime<Utc>, interval: u64) -> WeeklyQuotaRenderModel {
        let remaining = self
            .limits
            .as_ref()
            .and_then(|l| l.secondary.remaining_percent());
        let label = if let Some(f) = self.actionable_failure() {
            f.label().to_string()
        } else if self.stale(now, interval) {
            "数据过期".into()
        } else if let Some(l) = &self.limits {
            format!(
                "更新 {}",
                l.sampled_at.with_timezone(&Local).format("%H:%M")
            )
        } else {
            "等待查询".into()
        };
        WeeklyQuotaRenderModel {
            remaining_percent: remaining,
            percent_label: remaining
                .map(|p| format!("{p}%"))
                .unwrap_or_else(|| "--%".into()),
            title: "周剩余额度".into(),
            refresh_label: label,
        }
    }
    pub fn details(&self, now: DateTime<Utc>, interval: u64) -> String {
        let status = self.status_label(now, interval);
        let Some(l) = &self.limits else {
            return format!("状态：{status}\n只读监控，不发起 AI 请求");
        };
        let describe = |window: &crate::limits::LimitWindow| match window.remaining_percent() {
            Some(p) => format!(
                "剩余 {p}%{}",
                window
                    .resets_at
                    .map(|d| format!(" · {} 重置", d.with_timezone(&Local).format("%m-%d %H:%M")))
                    .unwrap_or_default()
            ),
            None => "接口未提供".into(),
        };
        format!(
            "状态：{status}\n周额度：{}\n五小时：{}\n最近成功：{}",
            describe(&l.secondary),
            describe(&l.primary),
            l.sampled_at.with_timezone(&Local).format("%m-%d %H:%M")
        )
    }
}
pub fn retry_delay(interval: u64, failures: u32) -> u64 {
    if failures == 0 {
        interval
    } else {
        match failures {
            1 => 5,
            2 => 15,
            3 => 30,
            n => 60u64.saturating_mul(1u64 << (n - 4).min(3)).min(300),
        }
    }
}
pub fn clamp_position(x: i32, y: i32, size: i32, work: (i32, i32, i32, i32)) -> (i32, i32) {
    let (left, top, right, bottom) = work;
    (
        x.clamp(left, (right - size).max(left)),
        y.clamp(top, (bottom - size).max(top)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::LimitWindow;
    #[test]
    fn transient_errors_keep_the_last_value_and_minute_time_without_a_failure_hint() {
        use chrono::TimeZone;
        let now = Local
            .with_ymd_and_hms(2026, 10, 15, 14, 25, 36)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let mut m = Monitor::default();
        m.succeeded(
            RateLimits {
                sampled_at: now,
                secondary: LimitWindow {
                    used_percent: Some(41),
                    ..Default::default()
                },
                ..Default::default()
            },
            "CLI".into(),
        );
        let before = m.render_model(now, 60);
        assert_eq!(before.refresh_label, "更新 14:25");
        for failure in [Failure::Network, Failure::Timeout] {
            m.failed(failure);
            assert_eq!(m.render_model(now, 60), before);
            assert!(!m.details(now, 60).contains("连接失败"));
            assert!(!m.details(now, 60).contains("查询超时"));
            assert!(m.details(now, 60).ends_with("最近成功：10-15 14:25"));
            assert_eq!(m.failure, Some(failure));
            assert_eq!(m.limits.as_ref().unwrap().sampled_at, now);
        }
        assert_eq!(
            m.render_model(now + chrono::Duration::minutes(4), 60)
                .refresh_label,
            "数据过期"
        );
    }
    #[test]
    fn updating_preserves_the_display_until_a_new_sample_succeeds() {
        let now = Utc::now();
        let mut m = Monitor::default();
        m.succeeded(
            RateLimits {
                sampled_at: now,
                secondary: LimitWindow {
                    used_percent: Some(34),
                    ..Default::default()
                },
                ..Default::default()
            },
            "CLI".into(),
        );
        let previous = m.render_model(now, 60);
        let details = m.details(now, 60);
        m.started();
        assert_eq!(m.render_model(now, 60), previous);
        assert_eq!(m.details(now, 60), details);
        let next = now + chrono::Duration::minutes(1);
        m.succeeded(
            RateLimits {
                sampled_at: next,
                secondary: LimitWindow {
                    used_percent: Some(34),
                    ..Default::default()
                },
                ..Default::default()
            },
            "CLI".into(),
        );
        assert_eq!(m.render_model(next, 60).remaining_percent, Some(66));
        assert_ne!(
            m.render_model(next, 60).refresh_label,
            previous.refresh_label
        );
        assert_eq!(m.limits.as_ref().unwrap().sampled_at, next);
    }
    #[test]
    fn transient_failure_preserves_last_success_until_its_age_expires() {
        let mut m = Monitor::default();
        let now = Utc::now();
        m.succeeded(
            RateLimits {
                secondary: LimitWindow {
                    used_percent: Some(34),
                    ..Default::default()
                },
                sampled_at: now,
                ..Default::default()
            },
            "CLI".into(),
        );
        m.failed(Failure::Network);
        assert_eq!(m.render_model(now, 60).percent_label, "66%");
        assert!(m.render_model(now, 60).refresh_label.starts_with("更新 "));
        assert!(!m.stale(now, 60));
        m.succeeded(
            RateLimits {
                sampled_at: now,
                ..Default::default()
            },
            "CLI".into(),
        );
        assert!(!m.stale(now, 60));
        assert!(m.failure.is_none());
    }
    #[test]
    fn absent_window_never_becomes_zero() {
        assert_eq!(
            Monitor::default()
                .render_model(Utc::now(), 60)
                .percent_label,
            "--%"
        );
    }
    #[test]
    fn ancient_sample_is_marked_expired() {
        let m = Monitor {
            limits: Some(RateLimits {
                sampled_at: Utc::now() - chrono::Duration::minutes(4),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(m.render_model(Utc::now(), 60).refresh_label, "数据过期");
    }
    #[test]
    fn backoff_has_a_bound() {
        assert_eq!(retry_delay(60, 0), 60);
        assert_eq!(retry_delay(60, 1), 5);
        assert_eq!(retry_delay(60, 10), 300);
        assert_eq!(retry_delay(300, 10), 300);
    }
    #[test]
    fn restores_a_removed_monitor_without_losing_negative_coordinates() {
        assert_eq!(
            clamp_position(-800, 20, 132, (-1920, 0, 0, 1080)),
            (-800, 20)
        );
        assert_eq!(
            clamp_position(4000, 4000, 132, (0, 0, 1920, 1080)),
            (1788, 948)
        );
    }
}
