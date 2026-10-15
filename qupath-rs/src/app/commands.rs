//! Every command, with QuPath's labels and shortcuts. Menus, shortcuts and
//! the command list are all generated from this table.

use egui::{Key, KeyboardShortcut, Modifiers};

macro_rules! commands {
    ($( $name:ident => $label:literal ),* $(,)?) => {
        /// A user command.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[allow(missing_docs)]
        pub enum Command { $($name,)* }

        impl Command {
            /// Every command.
            pub const ALL: &'static [Command] = &[$(Command::$name,)*];

            /// The menu label.
            pub fn label(self) -> &'static str {
                match self { $(Command::$name => $label,)* }
            }
        }
    };
}

commands! {
    OpenImage => "Open...",
    ProjectNew => "Create project",
    ProjectOpen => "Open project",
    ProjectClose => "Close project",
    ProjectAddImages => "Add images",
    SaveData => "Save",
    ReloadData => "Reload data",
    ExportOriginal => "Original pixels",
    ExportRendered => "Rendered RGB",
    ExportSnapshot => "Current viewer content",
    ImportObjects => "Import objects from file",
    ExportGeoJson => "Export objects as GeoJSON",
    Quit => "Quit",
    Undo => "Undo",
    Redo => "Redo",
    CopySelected => "Selected objects",
    CopyAnnotations => "Annotation objects",
    Paste => "Paste",
    Preferences => "Preferences...",
    ResetPreferences => "Reset preferences",
    ToolMove => "Move",
    ToolRectangle => "Rectangle",
    ToolEllipse => "Ellipse",
    ToolLine => "Line",
    ToolPolygon => "Polygon",
    ToolPolyline => "Polyline",
    ToolBrush => "Brush",
    ToolWand => "Wand",
    ToolPoints => "Points",
    SelectionMode => "Selection mode",
    ShowAnalysisPane => "Show analysis pane",
    CommandList => "Show command list",
    BrightnessContrast => "Brightness/Contrast",
    ZoomIn => "Zoom in",
    ZoomOut => "Zoom out",
    ZoomToFit => "Zoom to fit",
    Zoom400 => "400%",
    Zoom100 => "100%",
    Zoom10 => "10%",
    Zoom1 => "1%",
    ShowAnnotations => "Show annotations",
    FillAnnotations => "Fill annotations",
    ShowNames => "Show names",
    ShowDetections => "Show detections",
    FillDetections => "Fill detections",
    CellBoundaries => "Cell boundaries only",
    CellNuclei => "Nuclei only",
    CellBoth => "Nuclei & cell boundaries",
    CellCentroids => "Centroids only",
    ShowOverview => "Show slide overview",
    ShowLocation => "Show cursor location",
    ShowScalebar => "Show scalebar",
    ShowGrid => "Show counting grid",
    SlideLabel => "Show slide label",
    ShowLog => "Show log",
    DeleteSelected => "Delete selected objects",
    DeleteAnnotations => "Delete all annotations",
    DeleteDetections => "Delete all detections",
    DeleteAll => "Delete all objects",
    SelectAnnotations => "Select annotations",
    SelectDetections => "Select all detections",
    ResetSelection => "Reset selection",
    LockSelected => "Lock selected objects",
    UnlockSelected => "Unlock selected objects",
    ToggleLock => "Toggle selected objects locked",
    SpecifyAnnotation => "Specify annotation",
    FullImageAnnotation => "Create full image annotation",
    InsertIntoHierarchy => "Insert into hierarchy",
    ResolveHierarchy => "Resolve hierarchy",
    DuplicateAnnotation => "Duplicate annotations",
    ExpandAnnotations => "Expand annotations",
    SplitAnnotations => "Split annotations",
    MergeSelected => "Merge selected",
    SubtractSelected => "Subtract selected from main selection",
    MakeInverse => "Make inverse",
    SimplifyShape => "Simplify shape",
    CreateTiles => "Create tiles",
    CellDetection => "Cell detection",
    PositiveCellDetection => "Positive cell detection",
    CreateThresholder => "Create thresholder",
    ResetDetectionClassifications => "Reset detection classifications",
    AnnotationMeasurements => "Show annotation measurements",
    DetectionMeasurements => "Show detection measurements",
    ExportMeasurements => "Export measurements",
    About => "About qupath-rs",
    Documentation => "Documentation (QuPath)",
}

fn sc(modifiers: Modifiers, key: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(modifiers, key))
}

impl Command {
    /// QuPath's accelerator for the command.
    pub fn shortcut(self) -> Option<KeyboardShortcut> {
        use Command as C;
        let (cmd, shift, alt, none) = (Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::ALT, Modifiers::NONE);
        let cmd_shift = cmd | shift;
        let cmd_alt = cmd | alt;
        match self {
            C::OpenImage => sc(cmd, Key::O),
            C::SaveData => sc(cmd, Key::S),
            C::ReloadData => sc(cmd, Key::R),
            C::Quit => sc(cmd, Key::Q),
            C::Undo => sc(cmd, Key::Z),
            C::Redo => sc(cmd_shift, Key::Z),
            C::Preferences => sc(cmd, Key::Comma),
            C::ToolMove => sc(none, Key::M),
            C::ToolRectangle => sc(none, Key::R),
            C::ToolEllipse => sc(none, Key::O),
            C::ToolLine => sc(none, Key::L),
            C::ToolPolygon => sc(none, Key::P),
            C::ToolPolyline => sc(none, Key::V),
            C::ToolBrush => sc(none, Key::B),
            C::ToolWand => sc(none, Key::W),
            C::ToolPoints => sc(none, Key::Period),
            C::SelectionMode => sc(shift, Key::S),
            C::ShowAnalysisPane => sc(shift, Key::A),
            C::CommandList => sc(cmd, Key::L),
            C::BrightnessContrast => sc(shift, Key::C),
            C::ZoomIn => sc(none, Key::Equals),
            C::ZoomOut => sc(none, Key::Minus),
            C::ShowAnnotations => sc(none, Key::A),
            C::FillAnnotations => sc(shift, Key::F),
            C::ShowNames => sc(none, Key::N),
            C::ShowDetections => sc(none, Key::D),
            C::FillDetections => sc(none, Key::F),
            C::ShowGrid => sc(shift, Key::G),
            C::ShowLog => sc(cmd_shift, Key::L),
            C::DeleteSelected => sc(none, Key::Backspace),
            C::SelectAnnotations => sc(cmd_alt, Key::A),
            C::SelectDetections => sc(cmd_alt, Key::D),
            C::ResetSelection => sc(cmd_alt, Key::R),
            C::LockSelected => sc(cmd_shift, Key::K),
            C::UnlockSelected => sc(cmd_alt, Key::K),
            C::ToggleLock => sc(cmd, Key::K),
            C::FullImageAnnotation => sc(cmd_shift, Key::A),
            C::InsertIntoHierarchy => sc(cmd_shift, Key::I),
            C::ResolveHierarchy => sc(cmd_shift, Key::R),
            C::DuplicateAnnotation => sc(shift, Key::D),
            _ => None,
        }
    }

    /// Whether the shortcut is a bare key (or shift+key), which must not fire while typing.
    pub fn is_plain_key(self) -> bool {
        self.shortcut().is_some_and(|s| !s.modifiers.command && !s.modifiers.alt)
    }
}
