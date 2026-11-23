//! Logical-pixel layout shared by Start paint, input and popup placement.
pub(super) const INSET: i32 = 22;
pub(super) const HEADER_Y: i32 = 18;
pub(super) const HEADER_H: i32 = 36;
pub(super) const CONTROL: i32 = 32;
pub(super) const CONTROL_STEP: i32 = 38;
pub(super) const TABS_Y: i32 = 66;
pub(super) const CONTENT_Y: i32 = 101;
pub(super) const GRID_ROW: i32 = 77;
pub(super) const GRID_GAP: i32 = 5;
pub(super) const GRID_ICON: i32 = 46;
pub(super) const LIST_ROW: i32 = 36;
pub(super) const LIST_ICON: i32 = 23;
pub(super) const FOOTER_H: i32 = 50;
pub(super) const POWER: i32 = 38;
pub(super) const POWER_GAP: i32 = 12;
pub(super) const RESTART: i32 = 32;
pub(super) const POWER_CLUSTER: i32 = POWER + 6 + RESTART;
pub(super) const MAX_WIDTH: i32 = 544;
const MIN_HEIGHT: i32 = 388;
pub(super) const MAX_HEIGHT: i32 = 528;
const USAGE_H: i32 = 58;
pub(super) const ERROR_H: i32 = 20;

#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub width: i32,
    max_height: i32,
}

#[derive(Clone, Copy)]
pub(super) struct Flow {
    pub height: i32,
    pub recent_y: Option<i32>,
    pub frequent_y: Option<i32>,
    pub footer_y: i32,
}

impl Frame {
    pub fn new(available_width: i32, available_height: i32) -> Self {
        Self { width: available_width.clamp(272, MAX_WIDTH), max_height: available_height.clamp(248, MAX_HEIGHT) }
    }

    pub fn max_height(self) -> i32 { self.max_height }

    pub fn inner(self) -> i32 { self.width - 2 * INSET }

    pub fn search_width(self) -> i32 { self.inner() - 2 * CONTROL_STEP }

    pub fn columns(self, grid: bool) -> usize {
        if grid { (self.inner() as usize / 86).clamp(2, 5) } else { 1 }
    }

    pub fn capacity(self, grid: bool, recent: bool, error: bool) -> usize {
        let budget = self.max_height - CONTENT_Y - self.tail(recent, error);
        (budget / self.row(grid)).max(1) as usize * self.columns(grid)
    }

    fn row(self, grid: bool) -> i32 { if grid { GRID_ROW } else { LIST_ROW } }

    fn tail(self, recent: bool, error: bool) -> i32 {
        FOOTER_H + if recent { 2 * USAGE_H } else { 0 } + if error { ERROR_H } else { 0 }
    }

    pub fn show_recents(self, has_recents: bool, error: bool) -> bool {
        has_recents && CONTENT_Y + GRID_ROW + self.tail(true, error) <= self.max_height
    }

    pub fn flow(self, grid: bool, visible: usize, recent: bool, error: bool) -> Flow {
        let end = CONTENT_Y + (visible.div_ceil(self.columns(grid)) as i32 * self.row(grid)).max(36);
        let height = (end + self.tail(recent, error)).max(MIN_HEIGHT.min(self.max_height));
        Flow { height, recent_y: recent.then_some(end), frequent_y: recent.then_some(end + USAGE_H), footer_y: height - FOOTER_H }
    }

    pub fn app(self, index: usize, grid: bool) -> (i32, i32, i32, i32) {
        let columns = self.columns(grid);
        let x = index % columns;
        let a = self.inner() * x as i32 / columns as i32;
        let b = self.inner() * (x + 1) as i32 / columns as i32;
        (INSET + a, CONTENT_Y + (index / columns) as i32 * self.row(grid), b - a - GRID_GAP, self.row(grid) - 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn responsive_grid_and_search_fit_without_hiding_the_footer() {
        for width in [272, 360, 415, 544] {
            for height in [248, 290, 410, 528] {
                let frame = Frame::new(width, height);
                for grid in [false, true] {
                    for error in [false, true] {
                        let recent = grid && frame.show_recents(true, error);
                        let limit = frame.capacity(grid, recent, error);
                        let flow = frame.flow(grid, limit, recent, error);
                        assert!(flow.height <= height);
                        assert_eq!(flow.recent_y.is_some(), recent);
                        assert_eq!(flow.frequent_y.is_some(), recent);
                        assert!(flow.height >= height.min(MIN_HEIGHT));
                        let (x,y,w,h) = frame.app(limit-1,grid);
                        assert!(x+w <= width-INSET && y+h < flow.footer_y);
                    }
                }
            }
        }
        let wide = Frame::new(544,528);
        let narrow = Frame::new(300,528);
        assert!(wide.columns(true) > narrow.columns(true));
        assert!(wide.flow(true,2,true,false).height < wide.flow(true,15,true,false).height);
    }
}
