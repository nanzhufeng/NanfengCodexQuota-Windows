//! Compact native popup geometry adapted from the Mac context card.
use super::*;

pub(super) const WIDTH: i32 = 190;
pub(super) const HEIGHT: i32 = 166;

pub(super) fn anchor(click: POINT, work: RECT, dpi: u32) -> POINT {
    let s = |n| mul_div(n, dpi as i32, 96);
    POINT {
        x: (click.x + s(8)).clamp(work.left, (work.right - s(WIDTH)).max(work.left)),
        y: (click.y - s(8) - s(HEIGHT)).clamp(work.top, (work.bottom - s(HEIGHT)).max(work.top)),
    }
}

pub(super) fn centered(width: i32, height: i32, work: RECT) -> RECT {
    let width = width.min(work.right - work.left).max(1);
    let height = height.min(work.bottom - work.top).max(1);
    let left = work.left + (work.right - work.left - width) / 2;
    let top = work.top + (work.bottom - work.top - height) / 2;
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_uses_the_upper_right_anchor_and_stays_in_the_work_area() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        let p = anchor(POINT { x: 500, y: 500 }, work, 96);
        assert_eq!((p.x, p.y), (508, 326));
        let p = anchor(POINT { x: 1918, y: 10 }, work, 96);
        assert_eq!((p.x, p.y), (1730, 0));
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let p = anchor(POINT { x: -1000, y: 600 }, work, 96);
        assert_eq!((p.x, p.y), (-992, 426));
    }
    #[test]
    fn main_window_centers_and_fits_smaller_or_negative_coordinate_screens() {
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let r = centered(1080, 750, work);
        assert_eq!((r.left, r.top, r.right, r.bottom), (-1500, 145, -420, 895));
        let r = centered(
            1080,
            750,
            RECT {
                left: 0,
                top: 40,
                right: 1024,
                bottom: 740,
            },
        );
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 40, 1024, 740));
    }
}
