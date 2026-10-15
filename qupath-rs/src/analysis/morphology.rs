//! Filters and labelings on float images, for cell detection.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

/// A single-channel float image.
#[derive(Clone, Debug)]
pub struct FImage {
    /// Width.
    pub w: usize,
    /// Height.
    pub h: usize,
    /// Row-major pixels.
    pub data: Vec<f32>,
}

impl FImage {
    /// A blank image.
    pub fn new(w: usize, h: usize) -> Self {
        FImage { w, h, data: vec![0.0; w * h] }
    }
    /// Pixel value.
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.w + x]
    }
}

/// Gaussian smoothing.
pub fn gaussian(img: &FImage, sigma: f32) -> FImage {
    if sigma <= 0.0 {
        return img.clone();
    }
    let buf = image::ImageBuffer::<image::Luma<f32>, Vec<f32>>::from_raw(img.w as u32, img.h as u32, img.data.clone());
    let Some(buf) = buf else { return img.clone() };
    let out = imageproc::filter::gaussian_blur_f32(&buf, sigma);
    FImage { w: img.w, h: img.h, data: out.into_raw() }
}

/// Running min (or max) over a window of `2r+1` along rows or columns (van Herk/Gil-Werman).
fn running(img: &FImage, r: usize, horizontal: bool, max: bool) -> FImage {
    let pick = |a: f32, b: f32| if max { a.max(b) } else { a.min(b) };
    let (n_lines, len) = if horizontal { (img.h, img.w) } else { (img.w, img.h) };
    let idx = |line: usize, i: usize| if horizontal { line * img.w + i } else { i * img.w + line };
    let k = 2 * r + 1;
    let mut out = FImage::new(img.w, img.h);
    let fill = if max { f32::MIN } else { f32::MAX };
    let (mut g, mut hh) = (vec![fill; len + 2 * r + k], vec![fill; len + 2 * r + k]);
    for line in 0..n_lines {
        let padded: Vec<f32> = (0..len + 2 * r).map(|i| if i < r || i >= len + r { fill } else { img.data[idx(line, i - r)] }).collect();
        let n = padded.len();
        for i in 0..n {
            g[i] = if i % k == 0 { padded[i] } else { pick(g[i - 1], padded[i]) };
        }
        for i in (0..n).rev() {
            hh[i] = if i == n - 1 || (i + 1) % k == 0 { padded[i] } else { pick(hh[i + 1], padded[i]) };
        }
        for i in 0..len {
            out.data[idx(line, i)] = pick(hh[i], g[(i + 2 * r).min(n - 1)]);
        }
    }
    out
}

/// Square erosion then dilation: an estimate of the background under bright objects.
pub fn opening(img: &FImage, r: usize) -> FImage {
    if r == 0 {
        return img.clone();
    }
    let eroded = running(&running(img, r, true, false), r, false, false);
    running(&running(&eroded, r, true, true), r, false, true)
}

/// Local maxima above `threshold`, at least `min_dist` apart (strongest first).
pub fn find_seeds(img: &FImage, mask: &[bool], min_dist: f32) -> Vec<(usize, usize)> {
    let mut cands: Vec<(usize, usize, f32)> = Vec::new();
    for y in 1..img.h.saturating_sub(1) {
        for x in 1..img.w.saturating_sub(1) {
            let v = img.at(x, y);
            if !mask[y * img.w + x] {
                continue;
            }
            let is_max = (-1i32..=1).all(|dy| (-1i32..=1).all(|dx| img.at((x as i32 + dx) as usize, (y as i32 + dy) as usize) <= v));
            if is_max {
                cands.push((x, y, v));
            }
        }
    }
    cands.sort_by(|a, b| b.2.total_cmp(&a.2));
    let cell = (min_dist.max(1.0)) as usize;
    let (gw, gh) = (img.w / cell + 1, img.h / cell + 1);
    let mut grid: Vec<Vec<(usize, usize)>> = vec![Vec::new(); gw * gh];
    let mut seeds = Vec::new();
    let d2 = min_dist * min_dist;
    for (x, y, _) in cands {
        let (gx, gy) = (x / cell, y / cell);
        let near = (gy.saturating_sub(1)..=(gy + 1).min(gh - 1))
            .flat_map(|j| (gx.saturating_sub(1)..=(gx + 1).min(gw - 1)).map(move |i| (i, j)))
            .any(|(i, j)| grid[j * gw + i].iter().any(|(sx, sy)| ((*sx as f32 - x as f32).powi(2) + (*sy as f32 - y as f32).powi(2)) < d2));
        if !near {
            grid[gy * gw + gx].push((x, y));
            seeds.push((x, y));
        }
    }
    seeds
}

#[derive(PartialEq)]
struct Item(f32, usize, u32);
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0).then(o.1.cmp(&self.1))
    }
}

/// Marker watershed: floods from seeds through masked pixels, brightest first.
/// Returns labels (0 = background, seed i → i + 1).
pub fn watershed(img: &FImage, mask: &[bool], seeds: &[(usize, usize)]) -> Vec<u32> {
    let mut labels = vec![0u32; img.w * img.h];
    let mut heap = BinaryHeap::new();
    for (i, (x, y)) in seeds.iter().enumerate() {
        let p = y * img.w + x;
        labels[p] = i as u32 + 1;
        heap.push(Item(img.data[p], p, i as u32 + 1));
    }
    while let Some(Item(_, p, l)) = heap.pop() {
        let (x, y) = (p % img.w, p / img.w);
        for (nx, ny) in neighbors4(x, y, img.w, img.h) {
            let q = ny * img.w + nx;
            if mask[q] && labels[q] == 0 {
                labels[q] = l;
                heap.push(Item(img.data[q], q, l));
            }
        }
    }
    labels
}

fn neighbors4(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)].into_iter().filter(move |(a, b)| *a < w && *b < h)
}

/// Grows labels into unlabeled pixels up to `distance` (each pixel goes to the nearest label).
pub fn expand_labels(labels: &[u32], w: usize, h: usize, distance: f32) -> Vec<u32> {
    let mut out = labels.to_vec();
    if distance <= 0.0 {
        return out;
    }
    let mut dist = vec![f32::MAX; w * h];
    let mut queue = VecDeque::new();
    for (p, l) in labels.iter().enumerate() {
        if *l > 0 {
            dist[p] = 0.0;
            queue.push_back(p);
        }
    }
    // Chamfer distances (1, √2) in a breadth-first sweep.
    while let Some(p) = queue.pop_front() {
        let (x, y) = ((p % w) as i64, (p / w) as i64);
        for (dx, dy, c) in
            [(1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0), (1, 1, 1.414), (1, -1, 1.414), (-1, 1, 1.414), (-1, -1, 1.414)]
        {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                continue;
            }
            let q = ny as usize * w + nx as usize;
            let nd = dist[p] + c;
            if nd <= distance && nd < dist[q] {
                dist[q] = nd;
                out[q] = out[p];
                queue.push_back(q);
            }
        }
    }
    out
}

/// Labels of 4-connected components of a mask.
pub fn connected_components(mask: &[bool], w: usize, h: usize) -> Vec<u32> {
    let mut labels = vec![0u32; w * h];
    let mut next = 0u32;
    for start in 0..w * h {
        if !mask[start] || labels[start] != 0 {
            continue;
        }
        next += 1;
        labels[start] = next;
        let mut stack = vec![start];
        while let Some(p) = stack.pop() {
            for (nx, ny) in neighbors4(p % w, p / w, w, h) {
                let q = ny * w + nx;
                if mask[q] && labels[q] == 0 {
                    labels[q] = next;
                    stack.push(q);
                }
            }
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_removes_small_bright_spot() {
        let mut img = FImage::new(21, 21);
        img.data[10 * 21 + 10] = 5.0;
        let o = opening(&img, 2);
        assert!(o.data.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn watershed_splits_two_peaks() {
        let mut img = FImage::new(20, 5);
        for x in 0..20 {
            for y in 0..5 {
                img.data[y * 20 + x] = 1.0 - ((x as f32 - 5.0).abs().min((x as f32 - 14.0).abs()) / 10.0);
            }
        }
        let mask = vec![true; 100];
        let seeds = find_seeds(&img, &mask, 4.0);
        assert_eq!(seeds.len(), 2, "{seeds:?}");
        let l = watershed(&img, &mask, &seeds);
        assert_ne!(l[2 * 20 + 2], l[2 * 20 + 17]);
    }
}
