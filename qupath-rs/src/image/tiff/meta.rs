//! Reading calibration and channel names from TIFF descriptions (Aperio, OME-XML, resolution tags).

use super::ifd::{Ifd, tag};

/// What the description and tags say about the image.
#[derive(Clone, Debug, Default)]
pub struct DescribedMetadata {
    /// Pixel width in µm.
    pub pixel_width_um: Option<f64>,
    /// Pixel height in µm.
    pub pixel_height_um: Option<f64>,
    /// Objective magnification.
    pub magnification: Option<f64>,
    /// Number of OME channels stored as separate planes.
    pub ome_channels: Option<usize>,
    /// OME channel names.
    pub channel_names: Vec<String>,
    /// Format label for the Image tab.
    pub flavor: &'static str,
    /// Key/value pairs worth showing.
    pub properties: Vec<(String, String)>,
}

/// Reads metadata from the first IFD.
pub fn describe(ifd: &Ifd) -> DescribedMetadata {
    let desc = ifd.ascii(tag::IMAGE_DESCRIPTION).unwrap_or("");
    let mut m = if desc.starts_with("Aperio") {
        aperio(desc)
    } else if desc.contains("<OME") {
        ome(desc)
    } else {
        DescribedMetadata { flavor: "TIFF", ..Default::default() }
    };
    if m.pixel_width_um.is_none() {
        resolution_tags(ifd, &mut m);
    }
    if let Some(s) = ifd.ascii(tag::SOFTWARE) {
        m.properties.push(("Software".into(), s.to_owned()));
    }
    m
}

fn aperio(desc: &str) -> DescribedMetadata {
    let mut m = DescribedMetadata { flavor: "Aperio SVS", ..Default::default() };
    for part in desc.split('|').skip(1) {
        let Some((k, v)) = part.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "MPP" => {
                m.pixel_width_um = v.parse().ok();
                m.pixel_height_um = m.pixel_width_um;
            }
            "AppMag" => m.magnification = v.parse().ok(),
            _ => {}
        }
        m.properties.push((format!("aperio.{k}"), v.to_owned()));
    }
    m
}

/// Value of an XML attribute in the first element that has it.
pub fn xml_attr<'a>(xml: &'a str, element: &str, attr: &str) -> Option<&'a str> {
    let start = xml.find(&format!("<{element}"))?;
    let rest = &xml[start..];
    let end = rest.find('>')?;
    attr_in(&rest[..end], attr)
}

fn attr_in<'a>(tag_text: &'a str, attr: &str) -> Option<&'a str> {
    let key = format!(" {attr}=\"");
    let i = tag_text.find(&key)? + key.len();
    let j = tag_text[i..].find('"')?;
    Some(&tag_text[i..i + j])
}

fn ome(xml: &str) -> DescribedMetadata {
    let mut m = DescribedMetadata { flavor: "OME-TIFF", ..Default::default() };
    let to_um = |v: Option<&str>, unit: Option<&str>| -> Option<f64> {
        let v: f64 = v?.parse().ok()?;
        Some(match unit.unwrap_or("µm") {
            "nm" => v / 1000.0,
            "mm" => v * 1000.0,
            "cm" => v * 10000.0,
            _ => v,
        })
    };
    m.pixel_width_um = to_um(xml_attr(xml, "Pixels", "PhysicalSizeX"), xml_attr(xml, "Pixels", "PhysicalSizeXUnit"));
    m.pixel_height_um = to_um(xml_attr(xml, "Pixels", "PhysicalSizeY"), xml_attr(xml, "Pixels", "PhysicalSizeYUnit"));
    m.magnification = xml_attr(xml, "Objective", "NominalMagnification").and_then(|v| v.parse().ok());
    let size_c: Option<usize> = xml_attr(xml, "Pixels", "SizeC").and_then(|v| v.parse().ok());
    let mut names = Vec::new();
    let mut spp_total = 0usize;
    for (i, _) in xml.match_indices("<Channel ") {
        let end = xml[i..].find('>').map_or(xml.len(), |e| i + e);
        let t = &xml[i..end];
        names.push(attr_in(t, "Name").unwrap_or("").to_owned());
        spp_total += attr_in(t, "SamplesPerPixel").and_then(|v| v.parse().ok()).unwrap_or(1);
    }
    // Planes are separate when each channel has one sample.
    let n_named = names.len();
    m.ome_channels = size_c.filter(|c| *c > 1 && spp_total == *c && n_named == *c);
    m.channel_names = names;
    m
}

fn resolution_tags(ifd: &Ifd, m: &mut DescribedMetadata) {
    let unit = ifd.int(tag::RESOLUTION_UNIT).unwrap_or(2);
    let to_um = |res: f64| -> Option<f64> {
        match unit {
            3 if res > 0.0 => Some(10_000.0 / res),
            // Inches: only trust scanner-like resolutions, not 72/300 dpi defaults.
            2 if res > 1000.0 => Some(25_400.0 / res),
            _ => None,
        }
    };
    m.pixel_width_um = ifd.float(tag::X_RESOLUTION).and_then(to_um);
    m.pixel_height_um = ifd.float(tag::Y_RESOLUTION).and_then(to_um).or(m.pixel_width_um);
}

/// Name for a non-pyramid IFD (associated image).
pub fn associated_name(ifd: &Ifd, index: usize) -> String {
    let desc = ifd.ascii(tag::IMAGE_DESCRIPTION).unwrap_or("").to_lowercase();
    for n in ["label", "macro", "thumbnail", "overview"] {
        if desc.contains(n) {
            return n.to_owned();
        }
    }
    if index == 1 { "thumbnail".into() } else { format!("image {index}") }
}
