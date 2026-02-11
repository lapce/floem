use core::ops::Range;
use std::f64::consts::FRAC_PI_2;

use peniko::{Brush, kurbo::*};

pub struct BorderPath {
    path_iter: RoundedRectPathIter,
    range: Range<f64>,
}

impl BorderPath {
    pub fn new(rect: Rect, radii: RoundedRectRadii) -> Self {
        let rect = rect.abs();
        let shortest_side_length = (rect.width()).min(rect.height());
        let radii = radii.abs().clamp(shortest_side_length / 2.0);
        let rounded_path = RectPathIter {
            idx: 0,
            rect,
            radii,
        }
        .rounded_rect_path();

        Self {
            path_iter: rounded_path,
            range: 0.0..1.0,
        }
    }

    pub fn subsegment(&mut self, range: Range<f64>) {
        self.range = range;
    }

    pub fn path_elements<'a>(
        &'a mut self,
        strokes: &'a [(Stroke, Brush); 4],
        tolerance: f64,
    ) -> BorderPathIter<'a> {
        let total_len = self.path_iter.rect.total_len(tolerance);
        BorderPathIter {
            border_path: self,
            tolerance,
            current_len: 0.,
            current_iter: None,
            stroke_iter: strokes.iter().peekable(),
            emitted_last_stroke: false,
            total_len,
        }
    }
}

/// Returns a new Arc that represents a subsegment of this arc.
/// The range should be between 0.0 and 1.0, where:
/// - 0.0 represents the start of the original arc
/// - 1.0 represents the end of the original arc
pub fn arc_subsegment(arc: &Arc, range: Range<f64>) -> Arc {
    // Clamp the range to ensure it's within [0.0, 1.0]
    let start = range.start.clamp(0.0, 1.0);
    let end = range.end.clamp(0.0, 1.0);

    // Calculate the new start and sweep angles
    let total_sweep = arc.sweep_angle;
    let new_start_angle = arc.start_angle + total_sweep * start;
    let new_sweep_angle = total_sweep * (end - start);

    Arc {
        // These properties remain unchanged
        center: arc.center,
        radii: arc.radii,
        x_rotation: arc.x_rotation,
        // These are adjusted for the subsegment
        start_angle: new_start_angle,
        sweep_angle: new_sweep_angle,
    }
}

// First define the enum for our iterator output
pub enum BorderPathEvent<'a> {
    PathElement(PathEl),
    NewStroke(&'a (Stroke, Brush)),
}

pub struct BorderPathIter<'a> {
    border_path: &'a mut BorderPath,
    tolerance: f64,
    current_len: f64,
    current_iter: Option<Box<dyn Iterator<Item = PathEl> + 'a>>,
    stroke_iter: std::iter::Peekable<std::slice::Iter<'a, (Stroke, Brush)>>,
    emitted_last_stroke: bool,
    total_len: f64,
}

impl<'a> Iterator for BorderPathIter<'a> {
    type Item = BorderPathEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(
            self.total_len > 0.0,
            "Total length must be positive. Total_len: {}",
            self.total_len
        );
        assert!(
            self.current_len <= self.total_len,
            "Current length cannot exceed total length"
        );

        let end = self.border_path.range.end;
        assert!(end <= 1.0, "Range end must be <= 1.0");
        assert!(end >= 0.0, "Range end must be >= 0.0");

        // this large-ish epsilon value is necessary. If we check numbers too small then we have weird behavior where we don't properly check our end conditions when we reasonably should.
        const EPSILON: f64 = 1e-4;

        // Handle current iterator if it exists
        if let Some(iter) = &mut self.current_iter {
            if let Some(element) = iter.next() {
                return Some(BorderPathEvent::PathElement(element));
            }
            self.current_iter = None;
        }
        assert!(self.current_iter.is_none());

        // end condition: we have reached the target percentage
        if (self.current_len / self.total_len - end).abs() <= EPSILON
            || (self.current_len / self.total_len) >= end
        {
            return if self.emitted_last_stroke {
                None
            } else {
                self.emitted_last_stroke = true;
                assert!(
                    self.stroke_iter.size_hint().0 > 0,
                    "Must have exactly at least one stroke left. Strokes left: {}",
                    self.stroke_iter.size_hint().0
                );
                // return the last stroke then we will return
                self.stroke_iter.next().map(BorderPathEvent::NewStroke)
            };
        }

        loop {
            let next_seg = self.border_path.path_iter.next();

            match next_seg {
                Some(ArcOrPath::Arc(arc)) => {
                    // Set corner flag based on arc transition
                    let arc_len = arc.perimeter(self.tolerance);
                    if arc_len < EPSILON {
                        continue;
                    }
                    let normalized_current = self.current_len / self.total_len;
                    let normalized_seg_len = arc_len / self.total_len;

                    assert!(
                        normalized_seg_len > 0.0,
                        "Arc segment length must be positive"
                    );

                    if normalized_current + normalized_seg_len > end {
                        // Need to subsegment
                        let remaining_percentage = end - normalized_current;
                        let t = remaining_percentage / normalized_seg_len;
                        assert!(t > 0.0 && t <= 1.0, "Invalid subsegment parameter");

                        let subseg = arc_subsegment(&arc, 0.0..t);
                        let seg_len = subseg.perimeter(self.tolerance);
                        if seg_len < EPSILON {
                            continue;
                        }
                        self.current_len += seg_len;
                        self.current_iter = Some(Box::new(subseg.path_elements(self.tolerance)));
                    } else {
                        self.current_len += arc_len;
                        self.current_iter = Some(Box::new(arc.path_elements(self.tolerance)))
                    }
                    break;
                }
                Some(ArcOrPath::Path(path_seg)) => {
                    let seg_len = path_seg.arclen(self.tolerance);
                    if seg_len < EPSILON {
                        continue;
                    }
                    let normalized_current = self.current_len / self.total_len;
                    let normalized_seg_len = seg_len / self.total_len;

                    assert!(
                        normalized_seg_len > 0.0,
                        "Path segment length must be positive"
                    );

                    if normalized_current + normalized_seg_len > end {
                        let remaining_percentage = end - normalized_current;
                        let t = remaining_percentage / normalized_seg_len;
                        assert!(t > 0.0 && t <= 1.0, "Invalid subsegment parameter");

                        let subseg = path_seg.subsegment(0.0..t);

                        let seg_len = subseg.arclen(self.tolerance);
                        if seg_len < f64::EPSILON {
                            continue;
                        }
                        self.current_len += seg_len;
                        self.current_iter = Some(Box::new(subseg.path_elements(self.tolerance)));
                    } else {
                        self.current_len += seg_len;
                        self.current_iter = Some(Box::new(path_seg.path_elements(self.tolerance)));
                    }
                    break;
                }
                Some(ArcOrPath::Corner) => {
                    assert!(
                        self.stroke_iter.size_hint().0 > 0,
                        "Missing stroke at corner between arcs"
                    );

                    // Get the current stroke (advances iterator)
                    let current_stroke =
                        self.stroke_iter.next().expect("Another stroke is present");

                    // Peek at the next one (doesn't advance)
                    let next_stroke = self.stroke_iter.peek();

                    if let Some(&next) = next_stroke {
                        if current_stroke.0.width != next.0.width
                            || current_stroke.1 != next.1
                            || current_stroke.0.dash_pattern != next.0.dash_pattern
                        {
                            // Strokes are different, emit the current one
                            return Some(BorderPathEvent::NewStroke(current_stroke));
                        } else {
                            // need to return the next path element here without returning none so we let the loop go again.
                            // This is the only condition where the loop runs again
                        }
                    } else {
                        // No next stroke, we're at the end
                        self.emitted_last_stroke = true;
                        return Some(BorderPathEvent::NewStroke(current_stroke));
                    }
                }
                None => {
                    // at this point the *full* path has been emitted with all corners.
                    // If this occurs then the epsilon is too small. but honestly we can just return none and everything will be fine
                    return None;
                }
            }
        }

        // Get first element from new iterator
        if let Some(iter) = &mut self.current_iter {
            // We must have an element since we just created the iterator
            let el = iter
                .next()
                .unwrap_or_else(|| unreachable!("Empty iterator created"));
            Some(BorderPathEvent::PathElement(el))
        } else {
            unreachable!("should have returned a final stroke already")
        }
    }
}

// Taken from kurbo
#[derive(Debug)]
struct RectPathIter {
    rect: Rect,
    radii: RoundedRectRadii,
    idx: usize,
}

// This is clockwise in a y-down coordinate system for positive area.
impl Iterator for RectPathIter {
    type Item = PathSeg;
    fn next(&mut self) -> Option<PathSeg> {
        self.idx += 1;
        match self.idx {
            1 => Some(PathSeg::Line(Line::new(
                // Top edge - horizontal line
                Point::new(self.rect.x0 + self.radii.top_left, self.rect.y0), // Start after top-left corner
                Point::new(self.rect.x1 - self.radii.top_right, self.rect.y0), // End before top-right corner
            ))),
            2 => Some(PathSeg::Line(Line::new(
                // Right edge - vertical line
                Point::new(self.rect.x1, self.rect.y0 + self.radii.top_right), // Start after top-right corner
                Point::new(self.rect.x1, self.rect.y1 - self.radii.bottom_right), // End before bottom-right corner
            ))),
            3 => Some(PathSeg::Line(Line::new(
                // Bottom edge - horizontal line
                Point::new(self.rect.x1 - self.radii.bottom_right, self.rect.y1), // Start after bottom-right corner
                Point::new(self.rect.x0 + self.radii.bottom_left, self.rect.y1), // End before bottom-left corner
            ))),
            4 => Some(PathSeg::Line(Line::new(
                // Left edge - vertical line
                Point::new(self.rect.x0, self.rect.y1 - self.radii.bottom_left), // Start after bottom-left corner
                Point::new(self.rect.x0, self.rect.y0 + self.radii.top_left), // End before top-left corner
            ))),
            _ => None,
        }
    }
}

impl RectPathIter {
    fn build_corner_arc(&self, corner_idx: usize) -> Arc {
        let (center, radius) = match corner_idx {
            0 => (
                // top-left
                Point {
                    x: self.rect.x0 + self.radii.top_left,
                    y: self.rect.y0 + self.radii.top_left,
                },
                self.radii.top_left,
            ),
            1 => (
                // top-right
                Point {
                    x: self.rect.x1 - self.radii.top_right,
                    y: self.rect.y0 + self.radii.top_right,
                },
                self.radii.top_right,
            ),
            2 => (
                // bottom-right
                Point {
                    x: self.rect.x1 - self.radii.bottom_right,
                    y: self.rect.y1 - self.radii.bottom_right,
                },
                self.radii.bottom_right,
            ),
            3 => (
                // bottom-left
                Point {
                    x: self.rect.x0 + self.radii.bottom_left,
                    y: self.rect.y1 - self.radii.bottom_left,
                },
                self.radii.bottom_left,
            ),
            _ => unreachable!(),
        };

        Arc {
            center,
            radii: Vec2 {
                x: radius,
                y: radius,
            },
            start_angle: FRAC_PI_2 * ((corner_idx + 2) % 4) as f64,
            sweep_angle: FRAC_PI_2,
            x_rotation: 0.0,
        }
    }

    fn total_len(&self, tolerance: f64) -> f64 {
        // Calculate arc lengths with clamped radii
        let arc_lengths: f64 = (0..4)
            .map(|i| self.build_corner_arc(i).perimeter(tolerance))
            .sum();

        // Calculate straight segment lengths with clamped radii
        let straight_lengths = {
            let width = self.rect.width();
            let height = self.rect.height();
            let radii = &self.radii;
            // Top edge (minus the arc segments)
            let top = width - radii.top_left - radii.top_right;
            // Right edge
            let right = height - radii.top_right - radii.bottom_right;
            // Bottom edge
            let bottom = width - radii.bottom_left - radii.bottom_right;
            // Left edge
            let left = height - radii.top_left - radii.bottom_left;

            top + right + bottom + left
        };

        arc_lengths + straight_lengths
    }

    fn rounded_rect_path(&self) -> RoundedRectPathIter {
        // Note: order follows the rectangle path iterator.
        let arcs = [
            self.build_corner_arc(0),
            self.build_corner_arc(1),
            self.build_corner_arc(2),
            self.build_corner_arc(3),
        ];

        let arcs = [
            arc_subsegment(&arcs[0], 0.0..0.5),
            arc_subsegment(&arcs[0], 0.5..1.0),
            arc_subsegment(&arcs[1], 0.0..0.5),
            arc_subsegment(&arcs[1], 0.5..1.0),
            arc_subsegment(&arcs[2], 0.0..0.5),
            arc_subsegment(&arcs[2], 0.5..1.0),
            arc_subsegment(&arcs[3], 0.0..0.5),
            arc_subsegment(&arcs[3], 0.5..1.0),
        ];

        let rect = RectPathIter {
            rect: self.rect,
            idx: 0,
            radii: self.radii,
        };

        RoundedRectPathIter { idx: 0, rect, arcs }
    }
}

pub struct RoundedRectPathIter {
    idx: usize,
    rect: RectPathIter,
    arcs: [Arc; 8],
}

#[derive(Debug)]
pub enum ArcOrPath {
    Arc(Arc),
    Corner,
    Path(PathSeg),
}
// This is clockwise in a y-down coordinate system for positive area.
impl Iterator for RoundedRectPathIter {
    type Item = ArcOrPath;

    fn next(&mut self) -> Option<Self::Item> {
        // The total sequence is:
        // 0. Arc 1
        // 1. LineTo from rect
        // 2. Arc 2 (top right)
        // 3. Corner
        // 4. Arc 3 (right top)
        // 5. LineTo from rect
        // 6. Arc 4 (right bottom)
        // 7. Corner
        // 8. Arc 5 (bottom right)
        // 9. LineTo from rect
        // 10. Arc 6 (bottom left)
        // 11. Corner
        // 12. Arc 7 (left bottom)
        // 13. Final LineTo from rect
        // 14. Arc 8 (left top)
        // 15. Corner

        if self.idx >= 16 {
            return None;
        }

        let result = match self.idx {
            // Arc segments (even indices except 2,5,8)
            0 => Some(ArcOrPath::Arc(self.arcs[1])),
            2 => Some(ArcOrPath::Arc(self.arcs[2])),
            4 => Some(ArcOrPath::Arc(self.arcs[3])),
            6 => Some(ArcOrPath::Arc(self.arcs[4])),
            8 => Some(ArcOrPath::Arc(self.arcs[5])),
            10 => Some(ArcOrPath::Arc(self.arcs[6])),
            12 => Some(ArcOrPath::Arc(self.arcs[7])),
            14 => Some(ArcOrPath::Arc(self.arcs[0])),

            // Line segments (odd indices)
            1 | 5 | 9 | 13 => Some(ArcOrPath::Path(self.rect.next().unwrap())),

            3 | 7 | 11 | 15 => Some(ArcOrPath::Corner),

            16.. => None,
        };

        self.idx += 1;
        result
    }
}
