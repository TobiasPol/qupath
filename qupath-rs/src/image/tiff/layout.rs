//! Which IFDs form the pyramid (SVS, OME-TIFF SubIFDs, OME planes) and which are extras.

use anyhow::Result;

use super::ifd::{Ifd, TiffReader, tag};
use super::meta::{self, DescribedMetadata};

/// The IFDs in the main chain, followed by every SubIFD, with each SubIFD list per main IFD.
pub fn read_all_ifds(r: &mut TiffReader, first: u64) -> Result<(Vec<Ifd>, Vec<Vec<usize>>)> {
    let mut main = Vec::new();
    let mut off = first;
    while off != 0 && main.len() < super::limits::MAX_IFDS {
        let (ifd, next) = r.read_ifd(off)?;
        main.push(ifd);
        off = next;
    }
    let n_main = main.len();
    let mut all = main;
    let mut subs = vec![Vec::new(); n_main];
    for (i, sub) in subs.iter_mut().enumerate() {
        let offsets: Vec<u64> = all[i].ints(tag::SUB_IFDS).map(<[u64]>::to_vec).unwrap_or_default();
        for o in offsets {
            if all.len() >= super::limits::MAX_IFDS {
                anyhow::bail!("TIFF has more than {} directories", super::limits::MAX_IFDS);
            }
            let (ifd, _) = r.read_ifd(o)?;
            sub.push(all.len());
            all.push(ifd);
        }
    }
    Ok((all, subs))
}

/// Works out which IFDs make up the pyramid and which are extras.
pub fn layout(ifds: &[Ifd], subs: &[Vec<usize>], meta: &DescribedMetadata) -> (Vec<Vec<usize>>, Vec<(String, usize)>) {
    let n_main = subs.len();
    if !subs[0].is_empty() {
        // OME-TIFF: SubIFDs hold the lower resolutions, per plane.
        let planes: Vec<usize> = (0..meta.ome_channels.unwrap_or(1).min(n_main)).collect();
        let mut levels = vec![planes.clone()];
        for l in 0..subs[0].len() {
            levels.push(planes.iter().filter_map(|p| subs[*p].get(l).copied()).collect());
        }
        return (levels, Vec::new());
    }
    if let Some(c) = meta.ome_channels.filter(|c| *c <= n_main) {
        return (vec![(0..c).collect()], Vec::new());
    }
    let tiled = ifds[0].is_tiled();
    let mut levels = vec![vec![0]];
    let mut assoc = Vec::new();
    let spp = ifds[0].int(tag::SAMPLES_PER_PIXEL);
    for i in 1..n_main {
        let last = &ifds[*levels.last().and_then(|l| l.first()).unwrap_or(&0)];
        let is_level = ifds[i].is_tiled() == tiled
            && ifds[i].int(tag::SAMPLES_PER_PIXEL) == spp
            && ifds[i].width() < last.width()
            && ifds[i].height() < last.height()
            && (tiled || ifds[i].int(tag::NEW_SUBFILE_TYPE).unwrap_or(0) & 1 == 1)
            && !meta::associated_name(&ifds[i], i).starts_with(['l', 'm']);
        if is_level {
            levels.push(vec![i]);
        } else {
            assoc.push((meta::associated_name(&ifds[i], i), i));
        }
    }
    (levels, assoc)
}
