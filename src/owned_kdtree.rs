use std::collections::BinaryHeap;

use num_traits::{Float, One, Zero};

use crate::heap_element::HeapElement;
use crate::util::distance_to_space_const;

#[cfg_attr(feature = "serialize", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct OwnedKdTree<A, T: std::cmp::PartialEq, const D: usize> {
    // node
    left: Option<Box<OwnedKdTree<A, T, D>>>,
    right: Option<Box<OwnedKdTree<A, T, D>>>,
    // common
    capacity: usize,
    size: usize,
    min_bounds: [A; D],
    max_bounds: [A; D],
    // stem
    split_value: Option<A>,
    split_dimension: Option<usize>,
    // leaf
    points: Option<Vec<[A; D]>>,
    bucket: Option<Vec<T>>,
}

#[derive(Debug, PartialEq)]
pub enum ErrorKind {
    NonFiniteCoordinate,
    ZeroCapacity,
}

impl<A: Float + Zero + One, T: std::cmp::PartialEq, const D: usize> OwnedKdTree<A, T, D> {
    /// Create a new KD tree, specifying the dimension size of each point
    pub fn new() -> Self {
        OwnedKdTree::with_capacity(2_usize.pow(4))
    }

    /// Create a new KD tree, specifying the dimension size of each point and the capacity of leaf nodes
    pub fn with_capacity(capacity: usize) -> Self {
        let min_bounds = [A::infinity(); D];
        let max_bounds = [A::neg_infinity(); D];
        OwnedKdTree {
            left: None,
            right: None,
            capacity,
            size: 0,
            min_bounds: min_bounds,
            max_bounds: max_bounds,
            split_value: None,
            split_dimension: None,
            points: Some(vec![]),
            bucket: Some(vec![]),
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn nearest<F>(
        &self,
        point: &[A; D],
        num: usize,
        distance: &F,
    ) -> Result<Vec<(A, &T)>, ErrorKind>
    where
        F: Fn(&[A; D], &[A; D]) -> A,
    {
        let () = self.check_point(point)?;
        let num = std::cmp::min(num, self.size);
        if num == 0 {
            return Ok(vec![]);
        }
        let mut pending = BinaryHeap::new();
        let mut evaluated = BinaryHeap::<HeapElement<A, &T>>::new();
        pending.push(HeapElement {
            distance: A::zero(),
            element: self,
        });
        while !pending.is_empty()
            && (evaluated.len() < num
                || (-pending.peek().unwrap().distance <= evaluated.peek().unwrap().distance))
        {
            self.nearest_step(
                point,
                num,
                A::infinity(),
                distance,
                &mut pending,
                &mut evaluated,
            );
        }
        Ok(evaluated
            .into_sorted_vec()
            .into_iter()
            .take(num)
            .map(Into::into)
            .collect())
    }

    pub fn within<F>(
        &self,
        point: &[A; D],
        radius: A,
        distance: &F,
    ) -> Result<Vec<(A, &T)>, ErrorKind>
    where
        F: Fn(&[A; D], &[A; D]) -> A,
    {
        let () = self.check_point(point)?;
        if self.size == 0 {
            return Ok(vec![]);
        }
        let mut pending = BinaryHeap::new();
        let mut evaluated = BinaryHeap::<HeapElement<A, &T>>::new();
        pending.push(HeapElement {
            distance: A::zero(),
            element: self,
        });
        while !pending.is_empty() && (-pending.peek().unwrap().distance <= radius) {
            self.nearest_step(
                point,
                self.size,
                radius,
                distance,
                &mut pending,
                &mut evaluated,
            );
        }
        Ok(evaluated
            .into_sorted_vec()
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn nearest_step<'b, F>(
        &self,
        point: &[A; D],
        num: usize,
        max_dist: A,
        distance: &F,
        pending: &mut BinaryHeap<HeapElement<A, &'b Self>>,
        evaluated: &mut BinaryHeap<HeapElement<A, &'b T>>,
    ) where
        F: Fn(&[A; D], &[A; D]) -> A,
    {
        let mut curr = &*pending.pop().unwrap().element;
        debug_assert!(evaluated.len() <= num);
        let evaluated_dist = if evaluated.len() == num {
            // We only care about the nearest `num` points, so if we already have `num` points,
            // any more point we add to `evaluated` must be nearer then one of the point already in
            // `evaluated`.
            max_dist.min(evaluated.peek().unwrap().distance)
        } else {
            max_dist
        };

        while !curr.is_leaf() {
            let candidate;
            if curr.belongs_in_left(point) {
                candidate = curr.right.as_ref().unwrap();
                curr = curr.left.as_ref().unwrap();
            } else {
                candidate = curr.left.as_ref().unwrap();
                curr = curr.right.as_ref().unwrap();
            }
            let candidate_to_space = distance_to_space_const(
                point,
                &candidate.min_bounds,
                &candidate.max_bounds,
                distance,
            );
            if candidate_to_space <= evaluated_dist {
                pending.push(HeapElement {
                    distance: candidate_to_space * -A::one(),
                    element: &**candidate,
                });
            }
        }

        let points = curr.points.as_ref().unwrap().iter();
        let bucket = curr.bucket.as_ref().unwrap().iter();
        let iter = points.zip(bucket).map(|(p, d)| HeapElement {
            distance: distance(point, &p),
            element: d,
        });
        for element in iter {
            if element <= max_dist {
                if evaluated.len() < num {
                    evaluated.push(element);
                } else if element < *evaluated.peek().unwrap() {
                    evaluated.pop();
                    evaluated.push(element);
                }
            }
        }
    }

    pub fn iter_nearest<'a, 'b, F>(
        &'b self,
        point: &'a [A; D],
        distance: &'a F,
    ) -> Result<NearestIter<'a, 'b, A, T, F, D>, ErrorKind>
    where
        F: Fn(&[A; D], &[A; D]) -> A,
    {
        let () = self.check_point(point)?;
        let mut pending = BinaryHeap::new();
        let evaluated = BinaryHeap::<HeapElement<A, &T>>::new();
        pending.push(HeapElement {
            distance: A::zero(),
            element: self,
        });
        Ok(NearestIter {
            point,
            pending,
            evaluated,
            distance,
        })
    }

    pub fn iter_nearest_mut<'a, 'b, F>(
        &'b mut self,
        point: &'a [A; D],
        distance: &'a F,
    ) -> Result<NearestIterMut<'a, 'b, A, T, F, D>, ErrorKind>
    where
        F: Fn(&[A; D], &[A; D]) -> A,
    {
        let () = self.check_point(point)?;
        let mut pending = BinaryHeap::new();
        let evaluated = BinaryHeap::<HeapElement<A, &mut T>>::new();
        pending.push(HeapElement {
            distance: A::zero(),
            element: self,
        });
        Ok(NearestIterMut {
            point,
            pending,
            evaluated,
            distance,
        })
    }

    pub fn add(&mut self, point: [A; D], data: T) -> Result<(), ErrorKind> {
        if self.capacity == 0 {
            return Err(ErrorKind::ZeroCapacity);
        }
        let () = self.check_point(&point)?;
        self.add_unchecked(point, data)
    }

    fn add_unchecked(&mut self, point: [A; D], data: T) -> Result<(), ErrorKind> {
        if self.is_leaf() {
            self.add_to_bucket(point, data);
            return Ok(());
        }
        self.extend(&point);
        self.size += 1;
        let next = if self.belongs_in_left(&point) {
            self.left.as_mut()
        } else {
            self.right.as_mut()
        };
        next.unwrap().add_unchecked(point, data)
    }

    fn add_to_bucket(&mut self, point: [A; D], data: T) {
        self.extend(&point);
        let mut points = self.points.take().unwrap();
        let mut bucket = self.bucket.take().unwrap();
        points.push(point);
        bucket.push(data);
        self.size += 1;
        if self.size > self.capacity {
            self.split(points, bucket);
        } else {
            self.points = Some(points);
            self.bucket = Some(bucket);
        }
    }

    pub fn remove(&mut self, point: &[A; D], data: &T) -> Result<usize, ErrorKind> {
        let mut removed = 0;
        let () = self.check_point(&point)?;
        if let (Some(mut points), Some(mut bucket)) = (self.points.take(), self.bucket.take()) {
            while let Some(p_index) = points.iter().position(|x| x == point) {
                if &bucket[p_index] == data {
                    points.remove(p_index);
                    bucket.remove(p_index);
                    removed += 1;
                    self.size -= 1;
                }
            }
            self.points = Some(points);
            self.bucket = Some(bucket);
        } else {
            if let Some(right) = self.right.as_mut() {
                let right_removed = right.remove(point, data)?;
                if right_removed > 0 {
                    self.size -= right_removed;
                    removed += right_removed;
                }
            }
            if let Some(left) = self.left.as_mut() {
                let left_removed = left.remove(point, data)?;
                if left_removed > 0 {
                    self.size -= left_removed;
                    removed += left_removed;
                }
            }
        }
        Ok(removed)
    }

    fn split(&mut self, mut points: Vec<[A; D]>, mut bucket: Vec<T>) {
        let mut max = A::zero();
        for dim in 0..D {
            let diff = self.max_bounds[dim] - self.min_bounds[dim];
            if !diff.is_nan() && diff > max {
                max = diff;
                self.split_dimension = Some(dim);
            }
        }
        match self.split_dimension {
            None => {
                self.points = Some(points);
                self.bucket = Some(bucket);
                return;
            }
            Some(dim) => {
                let min = self.min_bounds[dim];
                let max = self.max_bounds[dim];
                self.split_value = Some(min + (max - min) / A::from(2.0).unwrap());
            }
        };
        let mut left = Box::new(OwnedKdTree::with_capacity(self.capacity));
        let mut right = Box::new(OwnedKdTree::with_capacity(self.capacity));
        while !points.is_empty() {
            let point = points.swap_remove(0);
            let data = bucket.swap_remove(0);
            if self.belongs_in_left(&point) {
                left.add_to_bucket(point, data);
            } else {
                right.add_to_bucket(point, data);
            }
        }
        self.left = Some(left);
        self.right = Some(right);
    }

    fn belongs_in_left(&self, point: &[A; D]) -> bool {
        point[self.split_dimension.unwrap()] < self.split_value.unwrap()
    }

    fn extend(&mut self, point: &[A; D]) {
        let min = self.min_bounds.iter_mut();
        let max = self.max_bounds.iter_mut();
        for ((l, h), v) in min.zip(max).zip(point.iter()) {
            if v < l {
                *l = *v
            }
            if v > h {
                *h = *v
            }
        }
    }

    fn is_leaf(&self) -> bool {
        self.bucket.is_some()
            && self.points.is_some()
            && self.split_value.is_none()
            && self.split_dimension.is_none()
            && self.left.is_none()
            && self.right.is_none()
    }

    fn check_point(&self, point: &[A; D]) -> Result<(), ErrorKind> {
        for n in point {
            if !n.is_finite() {
                return Err(ErrorKind::NonFiniteCoordinate);
            }
        }
        Ok(())
    }
}

pub struct NearestIter<
    'a,
    'b,
    A: 'a + 'b + Float,
    T: 'b + PartialEq,
    F: 'a + Fn(&[A; D], &[A; D]) -> A,
    const D: usize,
> {
    point: &'a [A; D],
    pending: BinaryHeap<HeapElement<A, &'b OwnedKdTree<A, T, D>>>,
    evaluated: BinaryHeap<HeapElement<A, &'b T>>,
    distance: &'a F,
}

impl<'a, 'b, A: Float + Zero + One, T: 'b, const D: usize, F: 'a> Iterator
    for NearestIter<'a, 'b, A, T, F, D>
where
    F: Fn(&[A; D], &[A; D]) -> A,
    T: PartialEq,
{
    type Item = (A, &'b T);
    fn next(&mut self) -> Option<(A, &'b T)> {
        let distance = self.distance;
        let point = self.point;
        while !self.pending.is_empty()
            && (self.evaluated.peek().map_or(A::infinity(), |x| -x.distance)
                >= -self.pending.peek().unwrap().distance)
        {
            let mut curr = &*self.pending.pop().unwrap().element;
            while !curr.is_leaf() {
                let candidate;
                if curr.belongs_in_left(point) {
                    candidate = curr.right.as_ref().unwrap();
                    curr = curr.left.as_ref().unwrap();
                } else {
                    candidate = curr.left.as_ref().unwrap();
                    curr = curr.right.as_ref().unwrap();
                }
                self.pending.push(HeapElement {
                    distance: -distance_to_space_const(
                        point,
                        &candidate.min_bounds,
                        &candidate.max_bounds,
                        distance,
                    ),
                    element: &**candidate,
                });
            }
            let points = curr.points.as_ref().unwrap().iter();
            let bucket = curr.bucket.as_ref().unwrap().iter();
            self.evaluated
                .extend(points.zip(bucket).map(|(p, d)| HeapElement {
                    distance: -distance(point, &p),
                    element: d,
                }));
        }
        self.evaluated.pop().map(|x| (-x.distance, x.element))
    }
}

pub struct NearestIterMut<
    'a,
    'b,
    A: 'a + 'b + Float,
    T: 'b + PartialEq,
    F: 'a + Fn(&[A; D], &[A; D]) -> A,
    const D: usize,
> {
    point: &'a [A; D],
    pending: BinaryHeap<HeapElement<A, &'b mut OwnedKdTree<A, T, D>>>,
    evaluated: BinaryHeap<HeapElement<A, &'b mut T>>,
    distance: &'a F,
}

impl<'a, 'b, A: Float + Zero + One, T: 'b, F: 'a, const D: usize> Iterator
    for NearestIterMut<'a, 'b, A, T, F, D>
where
    F: Fn(&[A; D], &[A; D]) -> A,
    T: PartialEq,
{
    type Item = (A, &'b mut T);
    fn next(&mut self) -> Option<(A, &'b mut T)> {
        let distance = self.distance;
        let point = self.point;
        while !self.pending.is_empty()
            && (self.evaluated.peek().map_or(A::infinity(), |x| -x.distance)
                >= -self.pending.peek().unwrap().distance)
        {
            let mut curr = &mut *self.pending.pop().unwrap().element;
            while !curr.is_leaf() {
                let candidate;
                if curr.belongs_in_left(point) {
                    candidate = curr.right.as_mut().unwrap();
                    curr = curr.left.as_mut().unwrap();
                } else {
                    candidate = curr.left.as_mut().unwrap();
                    curr = curr.right.as_mut().unwrap();
                }
                self.pending.push(HeapElement {
                    distance: -distance_to_space_const(
                        point,
                        &candidate.min_bounds,
                        &candidate.max_bounds,
                        distance,
                    ),
                    element: &mut **candidate,
                });
            }
            let points = curr.points.as_ref().unwrap().iter();
            let bucket = curr.bucket.as_mut().unwrap().iter_mut();
            self.evaluated
                .extend(points.zip(bucket).map(|(p, d)| HeapElement {
                    distance: -distance(point, &p),
                    element: d,
                }));
        }
        self.evaluated.pop().map(|x| (-x.distance, x.element))
    }
}

impl std::error::Error for ErrorKind {
    fn description(&self) -> &str {
        match *self {
            ErrorKind::NonFiniteCoordinate => "non-finite coordinate",
            ErrorKind::ZeroCapacity => "zero capacity",
        }
    }
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "KdTree error: {}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    extern crate rand;
    use super::KdTree;

    fn random_point() -> ([f64; 2], i32) {
        rand::random::<([f64; 2], i32)>()
    }

    #[test]
    fn it_has_default_capacity() {
        let tree: KdTree<f64, i32, [f64; 2]> = KdTree::new(2);
        assert_eq!(tree.capacity, 2_usize.pow(4));
    }

    #[test]
    fn it_can_be_cloned() {
        let mut tree: KdTree<f64, i32, [f64; 2]> = KdTree::new(2);
        let (pos, data) = random_point();
        tree.add(pos, data).unwrap();
        let mut cloned_tree = tree.clone();
        cloned_tree.add(pos, data).unwrap();
        assert_eq!(tree.size(), 1);
        assert_eq!(cloned_tree.size(), 2);
    }

    #[test]
    fn it_holds_on_to_its_capacity_before_splitting() {
        let mut tree: KdTree<f64, i32, [f64; 2]> = KdTree::new(2);
        let capacity = 2_usize.pow(4);
        for _ in 0..capacity {
            let (pos, data) = random_point();
            tree.add(pos, data).unwrap();
        }
        assert_eq!(tree.size, capacity);
        assert_eq!(tree.size(), capacity);
        assert!(tree.left.is_none() && tree.right.is_none());
        {
            let (pos, data) = random_point();
            tree.add(pos, data).unwrap();
        }
        assert_eq!(tree.size, capacity + 1);
        assert_eq!(tree.size(), capacity + 1);
        assert!(tree.left.is_some() && tree.right.is_some());
    }

    #[test]
    fn no_items_can_be_added_to_a_zero_capacity_kdtree() {
        let mut tree: KdTree<f64, i32, [f64; 2]> = KdTree::with_capacity(2, 0);
        let (pos, data) = random_point();
        let res = tree.add(pos, data);
        assert!(res.is_err());
    }
}