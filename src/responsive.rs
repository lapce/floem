use std::ops::{BitOr, Range, RangeBounds, RangeFrom, RangeTo};

use bitflags::bitflags;

bitflags! {
  #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
  #[must_use]
  pub struct SizeFlags: u16 {
    const XS = 1;
    const SM = 2;
    const MD = 4;
    const LG = 8;
    const XL = 16;
    const XXL = 32;
  }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum ScreenSizeBp {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
}

/// Width breakpoints in pixels
pub struct GridBreakpoints {
    xs: RangeTo<f64>,
    sm: Range<f64>,
    md: Range<f64>,
    lg: Range<f64>,
    xl: Range<f64>,
    xxl: RangeFrom<f64>,
}

impl Default for GridBreakpoints {
    fn default() -> Self {
        Self {
            xs: ..576.0,
            sm: 576.0..768.0,
            md: 768.0..992.0,
            lg: 992.0..1200.0,
            xl: 1200.0..1400.0,
            xxl: 1400.0..,
        }
    }
}

impl GridBreakpoints {
    pub(crate) fn get_width_bp(&self, width: f64) -> ScreenSizeBp {
        if self.xs.contains(&width) {
            return ScreenSizeBp::Xs;
        }
        if self.sm.contains(&width) {
            return ScreenSizeBp::Sm;
        }
        if self.md.contains(&width) {
            return ScreenSizeBp::Md;
        }
        if self.lg.contains(&width) {
            return ScreenSizeBp::Lg;
        }
        if self.xl.contains(&width) {
            return ScreenSizeBp::Xl;
        }
        if self.xxl.contains(&width) {
            return ScreenSizeBp::Xxl;
        }

        // This can only happen if breakpoint ranges are incorrect and have a gap
        panic!("Width {} did not match any breakpoint", width);
    }
}

fn next(size: ScreenSize) -> ScreenSize {
    ScreenSize {
        flags: SizeFlags::from_bits(size.flags.bits() * 2).unwrap(),
    }
}

fn prev(size: ScreenSize) -> ScreenSize {
    ScreenSize {
        flags: SizeFlags::from_bits(size.flags.bits() / 2).unwrap(),
    }
}

pub fn range<R: RangeBounds<ScreenSize>>(range: R) -> ScreenSize {
    let start = match range.start_bound() {
        std::ops::Bound::Included(i) => *i,
        std::ops::Bound::Excluded(e) => next(*e),
        std::ops::Bound::Unbounded => ScreenSize::XS,
    };
    let end = match range.end_bound() {
        std::ops::Bound::Included(s) => *s,
        std::ops::Bound::Excluded(e) => prev(*e),
        std::ops::Bound::Unbounded => ScreenSize::XXL,
    };
    // We get the first enabled flag from start and the last from the end.
    // This ensures that if a SizeFlag with multiple flags set(e.g. XS|SM|MD) is passed to the range,
    // it will still work correctly.
    let lowest_start: SizeFlags = start.flags.iter().next().unwrap();
    let highest_end: SizeFlags = end.flags.iter().last().unwrap();

    let mask = highest_end.bits() - lowest_start.bits();
    // Subtract to get all the flags between the two, and then OR to ensure everything in the range
    // is set.
    let result = SizeFlags::from_bits(highest_end.bits() | mask | lowest_start.bits()).unwrap();

    ScreenSize { flags: result }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenSize {
    flags: SizeFlags,
}

impl ScreenSize {
    pub const XS: ScreenSize = ScreenSize::new(SizeFlags::XS);
    pub const SM: ScreenSize = ScreenSize::new(SizeFlags::SM);
    pub const MD: ScreenSize = ScreenSize::new(SizeFlags::MD);
    pub const LG: ScreenSize = ScreenSize::new(SizeFlags::LG);
    pub const XL: ScreenSize = ScreenSize::new(SizeFlags::XL);
    pub const XXL: ScreenSize = ScreenSize::new(SizeFlags::XXL);

    const fn new(flags: SizeFlags) -> Self {
        Self { flags }
    }

    pub const fn not(size: ScreenSize) -> Self {
        let flags = SizeFlags::all().difference(size.flags);
        Self { flags }
    }

    pub(crate) fn breakpoints(&self) -> Vec<ScreenSizeBp> {
        let mut breakpoints = vec![];

        if self.flags.contains(SizeFlags::XS) {
            breakpoints.push(ScreenSizeBp::Xs);
        }

        if self.flags.contains(SizeFlags::SM) {
            breakpoints.push(ScreenSizeBp::Sm);
        }

        if self.flags.contains(SizeFlags::MD) {
            breakpoints.push(ScreenSizeBp::Md);
        }

        if self.flags.contains(SizeFlags::LG) {
            breakpoints.push(ScreenSizeBp::Lg);
        }

        if self.flags.contains(SizeFlags::XL) {
            breakpoints.push(ScreenSizeBp::Xl);
        }

        if self.flags.contains(SizeFlags::XXL) {
            breakpoints.push(ScreenSizeBp::Xxl);
        }

        breakpoints
    }
}

impl BitOr for ScreenSize {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::new(self.flags | rhs.flags)
    }
}

#[cfg(test)]
mod tests {
    use crate::responsive::SizeFlags;

    use super::{range, ScreenSize};

    #[test]
    fn range_full() {
        let size = range(ScreenSize::XS..=ScreenSize::XXL);
        assert!(size.flags.contains(SizeFlags::XS));
        assert!(size.flags.contains(SizeFlags::SM));
        assert!(size.flags.contains(SizeFlags::MD));
        assert!(size.flags.contains(SizeFlags::LG));
        assert!(size.flags.contains(SizeFlags::XL));
        assert!(size.flags.contains(SizeFlags::XXL));
    }

    #[test]
    fn union() {
        let size = ScreenSize::XS | ScreenSize::XL;
        assert!(size.flags.contains(SizeFlags::XS));
        assert!(size.flags.contains(SizeFlags::XL));

        assert!(!size.flags.contains(SizeFlags::SM));
        assert!(!size.flags.contains(SizeFlags::MD));
        assert!(!size.flags.contains(SizeFlags::LG));
        assert!(!size.flags.contains(SizeFlags::XXL));
    }

    #[test]
    fn xs_negated() {
        let size = ScreenSize::not(ScreenSize::XS);
        assert!(!size.flags.contains(SizeFlags::XS));

        assert!(size.flags.contains(SizeFlags::XL));
        assert!(size.flags.contains(SizeFlags::SM));
        assert!(size.flags.contains(SizeFlags::MD));
        assert!(size.flags.contains(SizeFlags::LG));
        assert!(size.flags.contains(SizeFlags::XXL));
    }

    #[test]
    fn negated_union() {
        let size = ScreenSize::not(ScreenSize::XS | ScreenSize::XL);
        assert!(!size.flags.contains(SizeFlags::XS));
        assert!(!size.flags.contains(SizeFlags::XL));

        assert!(size.flags.contains(SizeFlags::SM));
        assert!(size.flags.contains(SizeFlags::MD));
        assert!(size.flags.contains(SizeFlags::LG));
        assert!(size.flags.contains(SizeFlags::XXL));
    }

    #[test]
    fn range_xs2lg_incl() {
        let range = range(ScreenSize::XS..=ScreenSize::LG);
        assert!(range.flags.contains(SizeFlags::XS));
        assert!(range.flags.contains(SizeFlags::SM));
        assert!(range.flags.contains(SizeFlags::MD));
        assert!(range.flags.contains(SizeFlags::LG));

        assert!(!range.flags.contains(SizeFlags::XL));
        assert!(!range.flags.contains(SizeFlags::XXL));
    }

    #[test]
    fn range_xs2lg_excl() {
        let range = range(ScreenSize::XS..ScreenSize::LG);
        assert!(range.flags.contains(SizeFlags::XS));
        assert!(range.flags.contains(SizeFlags::SM));
        assert!(range.flags.contains(SizeFlags::MD));

        assert!(!range.flags.contains(SizeFlags::LG));
        assert!(!range.flags.contains(SizeFlags::XL));
        assert!(!range.flags.contains(SizeFlags::XXL));
    }

    #[test]
    fn range_overlapping_unions() {
        let small = ScreenSize::XS | ScreenSize::SM | ScreenSize::MD;
        let big = ScreenSize::MD | ScreenSize::LG;
        let size = range(small..=big);

        assert!(size.flags.contains(SizeFlags::XS));
        assert!(size.flags.contains(SizeFlags::SM));
        assert!(size.flags.contains(SizeFlags::MD));
        assert!(size.flags.contains(SizeFlags::LG));

        assert!(!size.flags.contains(SizeFlags::XL));
        assert!(!size.flags.contains(SizeFlags::XXL));
    }

    #[test]
    fn negated_range() {
        let range = ScreenSize::not(range(ScreenSize::XS..ScreenSize::LG));
        assert!(!range.flags.contains(SizeFlags::XS));
        assert!(!range.flags.contains(SizeFlags::SM));
        assert!(!range.flags.contains(SizeFlags::MD));

        assert!(range.flags.contains(SizeFlags::LG));
        assert!(range.flags.contains(SizeFlags::XL));
        assert!(range.flags.contains(SizeFlags::XXL));
    }
}
