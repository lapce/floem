#![allow(dead_code)]

use crate::defs::*;

pub struct Interval {
    pub a: f32,
    pub b: f32,
}

pub struct PathSegment {
    pub cvs: [LocalPoint; 3],
    pub next: Option<usize>,
    previous: Option<usize>,
}

impl PathSegment {
    pub fn new(a: LocalPoint, b: LocalPoint, c: LocalPoint) -> Self {
        Self {
            cvs: [a, b, c],
            next: None,
            previous: None,
        }
    }

    pub fn y_interval(&self) -> Interval {
        Interval {
            // Fatten the interval slightly to prevent artifacts by
            // slightly missing a curve in a band.
            a: self.cvs[0].y.min(self.cvs[1].y).min(self.cvs[2].y) - 1.0,
            b: self.cvs[0].y.max(self.cvs[1].y).max(self.cvs[2].y) + 1.0,
        }
    }
}

#[derive(PartialEq, PartialOrd)]
struct PathScannerNode {
    coord: f32,
    seg: usize,
    end: bool,
}

pub struct PathScanner {
    pub segments: Vec<PathSegment>,
    nodes: Vec<PathScannerNode>,
    index: usize,
    pub interval: Interval,
    pub first: Option<usize>,
}

impl PathScanner {
    pub fn new() -> Self {
        Self {
            segments: vec![],
            nodes: vec![],
            index: 0,
            interval: Interval { a: 0.0, b: 0.0 },
            first: None,
        }
    }

    pub fn init(&mut self) {
        // Close the path if necessary.
        if let Some(first) = self.segments.first() {
            if let Some(last) = self.segments.last() {
                let start = first.cvs[0];
                let end = last.cvs[2];
                if start != end {
                    self.segments.push(PathSegment {
                        cvs: [end, start.lerp(end, 0.5), start],
                        next: None,
                        previous: None,
                    })
                }
            }
        }

        self.nodes.clear();
        self.index = 0;

        for i in 0..self.segments.len() {
            let y_interval = self.segments[i].y_interval();
            self.nodes.push(PathScannerNode {
                coord: y_interval.a,
                seg: i,
                end: false,
            });
            self.nodes.push(PathScannerNode {
                coord: y_interval.b,
                seg: i,
                end: true,
            });
        }

        self.nodes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }

    pub fn begin(&mut self, cvs: &[LocalPoint]) {
        self.segments.clear();

        let mut i = 0;
        while i < cvs.len() - 2 {
            self.segments.push(PathSegment {
                cvs: [cvs[i], cvs[i + 1], cvs[i + 2]],
                next: None,
                previous: None,
            });
            i += 2;
        }

        self.init();
    }

    pub fn next(&mut self) -> bool {
        let y = self.nodes[self.index].coord;
        self.interval.a = y;
        let n = self.nodes.len();

        while self.index < n && self.nodes[self.index].coord == y {
            let node = &self.nodes[self.index];
            assert!(node.seg < self.segments.len());

            if node.end {
                if let Some(prev) = self.segments[node.seg].previous {
                    self.segments[prev].next = self.segments[node.seg].next;
                }
                if let Some(next) = self.segments[node.seg].next {
                    self.segments[next].previous = self.segments[node.seg].previous;
                }
                if self.first == Some(node.seg) {
                    self.first = self.segments[node.seg].next
                }
                self.segments[node.seg].next = None;
                self.segments[node.seg].previous = None;
            } else {
                self.segments[node.seg].next = self.first;
                if let Some(first) = self.first {
                    self.segments[first].previous = Some(node.seg);
                }
                self.first = Some(node.seg);
            }

            self.index += 1;
        }

        if self.index < n {
            self.interval.b = self.nodes[self.index].coord
        }

        self.index < n
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_path_scanner() {
        let mut scan = PathScanner::new();

        let cvs = vec![
            [1.0, 0.0].into(),
            [1.0, 1.0].into(),
            [0.0, 1.0].into(),
            [-1.0, 1.0].into(),
            [-1.0, 0.0].into(),
            [-1.0, -1.0].into(),
            [0.0, -1.0].into(),
            [1.0, -1.0].into(),
            [1.0, 0.0].into(),
        ];

        scan.begin(&cvs);

        assert_eq!(scan.segments.len(), 4);

        while scan.next() {
            print!(
                "interval {:?} {:?} active: ",
                scan.interval.a, scan.interval.b
            );

            let mut index = scan.first;
            while let Some(i) = index {
                print!("{:?} ", i);
                index = scan.segments[i].next;
            }

            println!();
        }
    }
}
