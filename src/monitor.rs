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
        self.failure.is_some()
            || self.limits.as_ref().is_some_and(|l| {
                (now - l.sampled_at).num_seconds() > (interval * 2).max(120) as i64
            })
    }
    pub fn render_model(&self, now: DateTime<Utc>, interval: u64) -> WeeklyQuotaRenderModel {
        let remaining = self
            .limits
            .as_ref()
            .and_then(|l| l.secondary.remaining_percent());
        let label = if let Some(f) = self.failure {
            f.label().to_string()
        } else if self.refreshing {
            "刷新中".into()
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
        let status = if let Some(f) = self.failure {
            f.label()
        } else if self.refreshing {
            "正在刷新"
        } else if self.stale(now, interval) {
            "数据过期"
        } else if self.limits.is_some() {
            "正常"
        } else {
            "等待查询"
        };
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
            l.sampled_at.with_timezone(&Local).format("%m-%d %H:%M:%S")
        )
    }
}
pub fn retry_delay(interval: u64, failures: u32) -> u64 {
    if failures == 0 {
        interval
    } else {
        interval.saturating_mul(1u64 << failures.min(3)).min(600)
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
    fn failure_preserves_last_success_and_marks_stale() {
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
        assert_eq!(m.render_model(now, 60).refresh_label, "连接失败");
        assert!(m.stale(now, 60));
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
        assert_eq!(retry_delay(60, 1), 120);
        assert_eq!(retry_delay(60, 10), 480);
        assert_eq!(retry_delay(300, 10), 600);
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
