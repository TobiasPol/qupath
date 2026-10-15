//! The menu bar layout, following QuPath's menus.

use super::commands::Command;

/// A menu item.
pub enum Item {
    /// A command.
    Cmd(Command),
    /// A separator.
    Sep,
    /// A submenu.
    Sub(&'static str, &'static [Item]),
}

use Command as C;
use Item::{Cmd, Sep, Sub};

/// The menu bar, following QuPath's layout.
pub const MENUS: &[(&str, &[Item])] = &[
    (
        "File",
        &[
            Sub("Project", &[Cmd(C::ProjectNew), Cmd(C::ProjectOpen), Cmd(C::ProjectClose), Sep, Cmd(C::ProjectAddImages)]),
            Sep,
            Cmd(C::OpenImage),
            Cmd(C::ReloadData),
            Sep,
            Cmd(C::SaveData),
            Sep,
            Sub("Export images...", &[Cmd(C::ExportOriginal), Cmd(C::ExportRendered)]),
            Sub("Export snapshot...", &[Cmd(C::ExportSnapshot)]),
            Sep,
            Cmd(C::ImportObjects),
            Cmd(C::ExportGeoJson),
            Sep,
            Cmd(C::Quit),
        ],
    ),
    (
        "Edit",
        &[
            Cmd(C::Undo),
            Cmd(C::Redo),
            Sep,
            Sub("Copy to clipboard...", &[Cmd(C::CopySelected), Cmd(C::CopyAnnotations)]),
            Cmd(C::Paste),
            Sep,
            Cmd(C::Preferences),
            Cmd(C::ResetPreferences),
        ],
    ),
    (
        "Tools",
        &[
            Cmd(C::ToolMove),
            Cmd(C::ToolRectangle),
            Cmd(C::ToolEllipse),
            Cmd(C::ToolLine),
            Cmd(C::ToolPolygon),
            Cmd(C::ToolPolyline),
            Cmd(C::ToolBrush),
            Cmd(C::ToolWand),
            Cmd(C::ToolPoints),
            Sep,
            Cmd(C::SelectionMode),
        ],
    ),
    (
        "View",
        &[
            Cmd(C::ShowAnalysisPane),
            Cmd(C::CommandList),
            Sep,
            Cmd(C::BrightnessContrast),
            Sep,
            Sub(
                "Zoom",
                &[Cmd(C::Zoom400), Cmd(C::Zoom100), Cmd(C::Zoom10), Cmd(C::Zoom1), Sep, Cmd(C::ZoomIn), Cmd(C::ZoomOut), Cmd(C::ZoomToFit)],
            ),
            Sep,
            Cmd(C::ShowAnnotations),
            Cmd(C::FillAnnotations),
            Cmd(C::ShowNames),
            Cmd(C::ShowDetections),
            Cmd(C::FillDetections),
            Sub("Cell display", &[Cmd(C::CellBoundaries), Cmd(C::CellNuclei), Cmd(C::CellBoth), Cmd(C::CellCentroids)]),
            Sep,
            Cmd(C::ShowOverview),
            Cmd(C::ShowLocation),
            Cmd(C::ShowScalebar),
            Cmd(C::ShowGrid),
            Sep,
            Cmd(C::SlideLabel),
            Sep,
            Cmd(C::ShowLog),
        ],
    ),
    (
        "Objects",
        &[
            Sub("Delete...", &[Cmd(C::DeleteSelected), Sep, Cmd(C::DeleteAnnotations), Cmd(C::DeleteDetections), Cmd(C::DeleteAll)]),
            Sub("Select...", &[Cmd(C::ResetSelection), Sep, Cmd(C::SelectAnnotations), Cmd(C::SelectDetections)]),
            Sub("Lock...", &[Cmd(C::LockSelected), Cmd(C::UnlockSelected), Cmd(C::ToggleLock)]),
            Sep,
            Sub(
                "Annotations...",
                &[
                    Cmd(C::SpecifyAnnotation),
                    Cmd(C::FullImageAnnotation),
                    Sep,
                    Cmd(C::InsertIntoHierarchy),
                    Cmd(C::ResolveHierarchy),
                    Sep,
                    Cmd(C::DuplicateAnnotation),
                    Cmd(C::ExpandAnnotations),
                    Cmd(C::SplitAnnotations),
                    Cmd(C::MergeSelected),
                    Cmd(C::SubtractSelected),
                    Cmd(C::MakeInverse),
                    Cmd(C::SimplifyShape),
                ],
            ),
        ],
    ),
    (
        "Analyze",
        &[
            Sub("Tiles & superpixels", &[Cmd(C::CreateTiles)]),
            Sub("Cell detection", &[Cmd(C::CellDetection), Cmd(C::PositiveCellDetection)]),
        ],
    ),
    (
        "Classify",
        &[
            Sub("Object classification", &[Cmd(C::ResetDetectionClassifications)]),
            Sub("Pixel classification", &[Cmd(C::CreateThresholder)]),
        ],
    ),
    ("Measure", &[Cmd(C::AnnotationMeasurements), Cmd(C::DetectionMeasurements), Sep, Cmd(C::ExportMeasurements)]),
    ("Help", &[Cmd(C::Documentation), Sep, Cmd(C::About)]),
];

/// The menu path of a command, e.g. "Objects › Annotations...".
pub fn menu_path(cmd: Command) -> String {
    fn find(items: &[Item], cmd: Command, path: &mut Vec<&'static str>) -> bool {
        for it in items {
            match it {
                Cmd(c) if *c == cmd => return true,
                Sub(name, sub) => {
                    path.push(name);
                    if find(sub, cmd, path) {
                        return true;
                    }
                    path.pop();
                }
                _ => {}
            }
        }
        false
    }
    for (name, items) in MENUS {
        let mut path = vec![*name];
        if find(items, cmd, &mut path) {
            return path.join(" › ");
        }
    }
    String::new()
}
