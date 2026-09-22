//! Defines the [PdfParagraph] struct, exposing functionality related to a group of
//! styled text strings that should be laid out together on a `PdfPage` as single paragraph.

use crate::bindgen::FPDF_PAGEOBJECT;
use crate::error::PdfiumError;
use crate::pdf::document::PdfDocument;
use crate::pdf::document::page::object::private::internal::PdfPageObjectPrivate;
use crate::pdf::document::page::object::text::PdfPageTextObject;
use crate::pdf::document::page::object::{PdfPageObject, PdfPageObjectCommon};
use crate::pdf::font::{PdfFont, PdfFontWeight};
use crate::pdf::points::PdfPoints;
use crate::pdf::quad_points::PdfQuadPoints;
use itertools::Itertools;
use maybe_owned::MaybeOwned;
use std::cmp::Ordering;

/// A page object annotated with the bounds (bottom, top, left, right) used to lay it out
/// within a [PdfParagraph]. ~keep
type PositionedObject<'a> = (PdfPoints, PdfPoints, PdfPoints, PdfPoints, &'a PdfPageObject<'a>);

/// Update an `Option<PdfPoints>` to track the minimum value seen.
fn update_min(slot: &mut Option<PdfPoints>, value: PdfPoints) {
    match *slot {
        Some(current) if current <= value => {}
        _ => *slot = Some(value),
    }
}

/// Update an `Option<PdfPoints>` to track the maximum value seen.
fn update_max(slot: &mut Option<PdfPoints>, value: PdfPoints) {
    match *slot {
        Some(current) if current >= value => {}
        _ => *slot = Some(value),
    }
}

/// A single styled string in a [PdfParagraph].
pub struct PdfStyledString<'a> {
    text: String,
    font: MaybeOwned<'a, PdfFont<'a>>,
    font_size: PdfPoints,
}

impl<'a> PdfStyledString<'a> {
    /// Creates a new [PdfStyledString] from the given arguments.
    #[inline]
    pub fn new(text: String, font: &'a PdfFont<'a>, font_size: PdfPoints) -> Self {
        PdfStyledString {
            text,
            font: MaybeOwned::Borrowed(font),
            font_size,
        }
    }

    /// Creates a new [PdfStyledString] from the given [PdfPageTextObject].
    #[inline]
    pub fn from_text_object(text_object: &'a PdfPageTextObject<'a>) -> Self {
        PdfStyledString {
            text: text_object.text(),
            font: MaybeOwned::Owned(text_object.font()),
            font_size: text_object.unscaled_font_size(),
        }
    }

    /// Adds the given string to the text in this [PdfStyledString]. The given separator will be used
    /// to separate the existing text in this [PdfStyledString] from the given string.
    #[inline]
    pub(crate) fn push(&mut self, text: impl ToString, separator: &str) {
        if !self.text.ends_with(separator) {
            self.text.push_str(separator);
        }

        self.text.push_str(text.to_string().as_str());
    }

    /// Returns the text in this [PdfStyledString].
    #[inline]
    pub fn text(&self) -> &str {
        self.text.as_str()
    }

    /// Returns the [PdfFont] used to style this [PdfStyledString].
    #[inline]
    pub fn font(&self) -> &PdfFont<'_> {
        self.font.as_ref()
    }

    /// Returns the font size used to style this [PdfStyledString].
    #[inline]
    pub fn font_size(&self) -> PdfPoints {
        self.font_size
    }

    /// Returns `true` if the font and font size of this [PdfStyledString] is the same as
    /// that of the given string.
    #[inline]
    pub fn does_match_string_styling(&self, other: &PdfStyledString) -> bool {
        self.does_match_raw_styling(other.font_size(), other.font())
    }

    /// Returns `true` if the font and font size of this [PdfStyledString] is the same as
    /// that of the given [PdfPageTextObject].
    #[inline]
    pub fn does_match_object_styling(&self, other: &PdfPageTextObject) -> bool {
        self.does_match_raw_styling(other.unscaled_font_size(), &other.font())
    }

    /// Returns `true` if this styled string's font is bold.
    ///
    /// Checks the font descriptor's force-bold flag, the font weight (>= 700),
    /// and the font family name for "bold" substring.
    pub fn is_bold(&self) -> bool {
        let font = self.font();

        if font.is_bold_reenforced() {
            return true;
        }

        if let Ok(weight) = font.weight()
            && matches!(
                weight,
                PdfFontWeight::Weight700Bold | PdfFontWeight::Weight800 | PdfFontWeight::Weight900
            )
        {
            return true;
        }

        font.family().to_lowercase().contains("bold")
    }

    /// Returns `true` if this styled string's font is italic.
    ///
    /// Checks the font descriptor's italic flag and the font family name
    /// for "italic" or "oblique" substrings.
    pub fn is_italic(&self) -> bool {
        let font = self.font();

        if font.is_italic() {
            return true;
        }

        let name = font.family().to_lowercase();
        name.contains("italic") || name.contains("oblique")
    }

    /// Returns `true` if this styled string's font is monospace.
    ///
    /// Checks the font descriptor's fixed-pitch flag and the font family name
    /// against common monospace font patterns.
    pub fn is_monospace(&self) -> bool {
        let font = self.font();

        if font.is_fixed_pitch() {
            return true;
        }

        let name = font.family().to_lowercase();
        const MONOSPACE_PATTERNS: &[&str] = &[
            "mono",
            "courier",
            "consolas",
            "menlo",
            "source code",
            "inconsolata",
            "fira code",
            "liberation mono",
            "lucida console",
            "andale mono",
            "dejavu sans mono",
            "roboto mono",
            "noto mono",
            "ibm plex mono",
            "jetbrains mono",
            "cascadia",
            "hack",
        ];
        MONOSPACE_PATTERNS.iter().any(|p| name.contains(p))
    }

    fn does_match_raw_styling(&self, other_font_size: PdfPoints, other_font: &PdfFont) -> bool {
        if self.font_size() != other_font_size {
            return false;
        }

        let this_font = self.font();

        if this_font.handle() != other_font.handle() {
            return false;
        }

        let this_font_name = this_font.family();

        let other_font_name = other_font.family();

        if this_font_name.is_empty() && other_font_name.is_empty() {
            return true;
        }

        (!this_font_name.is_empty() || !other_font_name.is_empty()) && this_font_name == other_font_name
    }

    /// Creates a new [PdfPageTextObject] from this styled string, using the Pdfium bindings in
    /// the given document.
    #[inline]
    pub fn as_text_object(&self, document: &PdfDocument<'a>) -> Result<PdfPageTextObject<'a>, PdfiumError> {
        PdfPageTextObject::new(document, self.text(), self.font(), self.font_size())
    }
}

/// A single fragment in a [PdfParagraph]. The fragment may later be split into sub-fragments when
/// assembling the [PdfParagraph] into lines.
pub enum PdfParagraphFragment<'a> {
    /// A run of styled text.
    StyledString(PdfStyledString<'a>),
    /// A line break with alignment and position information from the preceding line.
    LineBreak {
        alignment: PdfLineAlignment,
        bottom: PdfPoints,
        left: PdfPoints,
    },
    /// A non-text page object (image, path, shading, etc.).
    NonTextObject(FPDF_PAGEOBJECT),
}

/// Controls the line alignment behaviour of a [PdfParagraph].
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PdfParagraphAlignment {
    /// All lines will be non-justified, aligned to the left.
    LeftAlign,

    /// All lines will be non-justified, aligned to the right.
    RightAlign,

    /// All lines will be non-justified and centered.
    Center,

    /// All lines except the last will be justified.
    Justify,

    /// All lines, including the last, will be justified.
    ForceJustify,
}

/// The paragraph-relative alignment of a single [PdfLine].
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PdfLineAlignment {
    /// No alignment detected.
    None,
    /// Left-aligned.
    LeftAlign,
    /// Right-aligned.
    RightAlign,
    /// Centered.
    Center,
    /// Justified.
    Justify,
}

/// A span of paragraph fragments that make up one line in a [PdfParagraph].
pub struct PdfLine<'a> {
    /// The alignment of this line within the paragraph.
    pub alignment: PdfLineAlignment,
    /// The bottom Y position of this line in PDF points.
    pub bottom: PdfPoints,
    /// The left X position of this line in PDF points.
    pub left: PdfPoints,
    /// The width of this line in PDF points.
    pub width: PdfPoints,
    /// The fragments composing this line.
    pub fragments: Vec<PdfParagraphFragment<'a>>,
}

impl<'a> PdfLine<'a> {
    #[inline]
    fn new(
        alignment: PdfLineAlignment,
        bottom: PdfPoints,
        left: PdfPoints,
        width: PdfPoints,
        fragments: Vec<PdfParagraphFragment<'a>>,
    ) -> Self {
        PdfLine {
            alignment,
            bottom,
            left,
            width,
            fragments,
        }
    }
}

/// A group of [PdfPageTextObject] objects contained in the same `PdfPageObjects` collection
/// that should be laid out together as a single paragraph.
///
/// Text layout in PDF files is handled entirely by text objects. Each text object contains
/// a single span of text that is styled consistently and can be at most a single line long.
/// Multiple text objects stitched together visually at the time the page is generated are
/// interpreted by the reader as paragraphs, but there is no concept in the PDF file format
/// of a multi-line text block, and there is no native functionality for retrieving a single
/// paragraph from its constituent text objects. This makes it difficult to work with long spans
/// of text.
///
/// The [PdfParagraph] is an attempt to improve multi-line text handling. Paragraphs can
/// be created from existing groups of page objects, or created by scratch; once created, text in
/// a paragraph can be edited and re-formatted, and then used to generate a group of text objects
/// that can be placed on a page.
pub struct PdfParagraph<'a> {
    fragments: Vec<PdfParagraphFragment<'a>>,
    bottom: Option<PdfPoints>,
    left: Option<PdfPoints>,
    max_width: Option<PdfPoints>,
    alignment: PdfParagraphAlignment,
}

impl<'a> PdfParagraph<'a> {
    // ~keep TODO: lifetime issues, using iterator is a possibility but PdfPage::objects().iter()
    // ~keep and PdfPageGroupObject::iter() return iterators over PdfPageObject<'a> whereas
    // ~keep &[PdfPageObject<'a>] returns an iterator over &PdfPageObject<'a>

    // #[inline]
    // #[inline]

    /// Creates a set of one or more [PdfParagraph] objects from the given slice of page objects.
    pub fn from_objects(objects: &'a [PdfPageObject<'a>]) -> Vec<PdfParagraph<'a>> {
        let (positioned_objects, paragraph_left, paragraph_right) =
            Self::positioned_objects_sorted_and_filtered(objects);

        let lines = Self::assemble_lines(&positioned_objects, paragraph_left, paragraph_right);

        Self::group_lines_into_paragraphs(lines)
    }

    /// Computes the bounds of each of the given page objects, sorts them into reading order
    /// (top to bottom, then left to right), and filters out significantly-rotated non-text
    /// objects. Also returns the leftmost and rightmost extents of the given objects, used as
    /// the paragraph's overall bounds. ~keep
    fn positioned_objects_sorted_and_filtered(
        objects: &'a [PdfPageObject<'a>],
    ) -> (Vec<PositionedObject<'a>>, PdfPoints, PdfPoints) {
        let mut objects_bottom = None;

        let mut objects_top = None;

        let mut objects_left = None;

        let mut objects_right = None;

        let positioned_objects = objects
            .iter()
            .map(|object| {
                let bounds = object.bounds().ok();

                let object_bottom = bounds.map(|b| b.bottom()).unwrap_or(PdfPoints::ZERO);
                let object_top = bounds.map(|b| b.top()).unwrap_or(PdfPoints::ZERO);
                let object_left = bounds.map(|b| b.left()).unwrap_or(PdfPoints::ZERO);
                let object_right = bounds.map(|b| b.right()).unwrap_or(PdfPoints::ZERO);

                update_min(&mut objects_bottom, object_bottom);
                update_max(&mut objects_top, object_top);
                update_min(&mut objects_left, object_left);
                update_max(&mut objects_right, object_right);

                (object_bottom, object_top, object_left, object_right, object)
            })
            .sorted_by(|a, b| {
                let (a_top, a_left) = (a.1, a.2);
                let (b_top, b_left) = (b.1, b.2);

                match b_top.value.total_cmp(&a_top.value) {
                    Ordering::Equal => a_left.value.total_cmp(&b_left.value),
                    other => other,
                }
            })
            .collect::<Vec<_>>();

        let positioned_objects: Vec<_> = positioned_objects
            .into_iter()
            .filter(|(_, _, _, _, object)| object.as_text_object().is_none() || !is_significantly_rotated(object))
            .collect();

        let paragraph_left = objects_left.unwrap_or(PdfPoints::ZERO);
        let paragraph_right = objects_right.unwrap_or(paragraph_left);

        (positioned_objects, paragraph_left, paragraph_right)
    }

    /// Assembles the given reading-order-sorted page objects into a sequence of [PdfLine]s,
    /// starting a new line whenever the alignment changes or a vertical gap is detected. ~keep
    fn assemble_lines(
        positioned_objects: &[PositionedObject<'a>],
        paragraph_left: PdfPoints,
        paragraph_right: PdfPoints,
    ) -> Vec<PdfLine<'a>> {
        let mut lines = Vec::new();
        let mut current = CurrentLine::new();
        let mut last_object = LastObjectInfo::default();

        for (bottom, top, left, right, object) in positioned_objects.iter() {
            let bounds = ObjectBounds {
                bottom: *bottom,
                top: *top,
                left: *left,
                right: *right,
            };

            maybe_start_new_line(
                &mut lines,
                &mut current,
                &last_object,
                bounds,
                paragraph_left,
                paragraph_right,
            );

            last_object = LastObjectInfo {
                left: Some(bounds.left),
                right: Some(bounds.right),
                bottom: Some(bounds.bottom),
                height: Some(bounds.top - bounds.bottom),
            };

            current.right = append_object_to_line(
                &mut current.fragments,
                object,
                bounds.right,
                last_object.right,
                current.right,
            );
        }

        lines.push(PdfLine::new(
            current.alignment,
            current.bottom,
            current.left,
            current.right - current.left,
            current.fragments,
        ));

        lines
    }

    /// Groups the given sequence of [PdfLine]s into one or more [PdfParagraph]s, starting a new
    /// paragraph whenever the line alignment changes. ~keep
    fn group_lines_into_paragraphs(mut lines: Vec<PdfLine<'a>>) -> Vec<PdfParagraph<'a>> {
        let mut paragraphs = Vec::new();

        let mut current_paragraph_fragments = Vec::new();

        let mut current_paragraph_bottom = None;

        let mut current_paragraph_left = None;

        let mut current_paragraph_right = None;

        let mut last_line_alignment = lines
            .first()
            .map(|line| line.alignment)
            .unwrap_or(PdfLineAlignment::None);

        let mut first_line_alignment = last_line_alignment;

        for mut line in lines.drain(..) {
            if line.alignment != last_line_alignment {
                // ~keep TODO: this won't work as expected for non-force-justified paragraphs
                // ~keep where the last line in the paragraph is left-aligned, not justified

                if !current_paragraph_fragments.is_empty() {
                    paragraphs.push(Self::paragraph_from_lines(
                        current_paragraph_fragments,
                        current_paragraph_bottom,
                        current_paragraph_left,
                        current_paragraph_right,
                        first_line_alignment,
                        last_line_alignment,
                    ));

                    current_paragraph_fragments = Vec::new();
                    current_paragraph_bottom = None;
                    current_paragraph_left = None;
                    current_paragraph_right = None;
                    first_line_alignment = last_line_alignment
                }
            }

            current_paragraph_fragments.append(&mut line.fragments);

            last_line_alignment = line.alignment;

            update_min(&mut current_paragraph_left, line.left);
            update_max(&mut current_paragraph_right, line.left + line.width);
            update_min(&mut current_paragraph_bottom, line.bottom);
        }

        paragraphs.push(Self::paragraph_from_lines(
            current_paragraph_fragments,
            current_paragraph_bottom,
            current_paragraph_left,
            current_paragraph_right,
            first_line_alignment,
            last_line_alignment,
        ));

        paragraphs
    }

    fn paragraph_from_lines(
        fragments: Vec<PdfParagraphFragment<'a>>,
        bottom: Option<PdfPoints>,
        left: Option<PdfPoints>,
        right: Option<PdfPoints>,
        first_line_alignment: PdfLineAlignment,
        last_line_alignment: PdfLineAlignment,
    ) -> PdfParagraph<'a> {
        PdfParagraph {
            fragments,
            bottom,
            left,
            max_width: match (left, right) {
                (Some(left), Some(right)) => Some(right - left),
                _ => None,
            },
            alignment: if first_line_alignment == last_line_alignment
                && first_line_alignment == PdfLineAlignment::Justify
            {
                PdfParagraphAlignment::ForceJustify
            } else {
                match first_line_alignment {
                    PdfLineAlignment::None | PdfLineAlignment::LeftAlign => PdfParagraphAlignment::LeftAlign,
                    PdfLineAlignment::RightAlign => PdfParagraphAlignment::RightAlign,
                    PdfLineAlignment::Center => PdfParagraphAlignment::Center,
                    PdfLineAlignment::Justify => PdfParagraphAlignment::Justify,
                }
            },
        }
    }

    fn guess_line_alignment(
        previous_line_left: Option<PdfPoints>,
        previous_line_right: Option<PdfPoints>,
        line_left: PdfPoints,
        line_right: PdfPoints,
        paragraph_left: PdfPoints,
        paragraph_right: PdfPoints,
    ) -> PdfLineAlignment {
        const ALIGNMENT_THRESHOLD: f32 = 2.0;

        if let (Some(previous_line_left), Some(previous_line_right)) = (previous_line_left, previous_line_right) {
            let is_aligned_left = (previous_line_left.value - line_left.value).abs() < ALIGNMENT_THRESHOLD;

            let is_aligned_right = (previous_line_right.value - line_right.value).abs() < ALIGNMENT_THRESHOLD;

            match (is_aligned_left, is_aligned_right) {
                (true, true) => PdfLineAlignment::Justify,
                (true, false) => PdfLineAlignment::LeftAlign,
                (false, true) => PdfLineAlignment::RightAlign,
                (false, false) => PdfLineAlignment::Center,
            }
        } else {
            let is_aligned_left = (paragraph_left.value - line_left.value).abs() < ALIGNMENT_THRESHOLD;

            let is_aligned_right = (paragraph_right.value - line_right.value).abs() < ALIGNMENT_THRESHOLD;

            match (is_aligned_left, is_aligned_right) {
                (true, true) => PdfLineAlignment::Justify,
                (true, false) => PdfLineAlignment::LeftAlign,
                (false, true) => PdfLineAlignment::RightAlign,
                (false, false) => PdfLineAlignment::Center,
            }
        }
    }

    /// Creates a new, empty [PdfParagraph] with the given maximum line width
    /// and alignment settings.
    #[inline]
    pub fn empty(maximum_width: PdfPoints, alignment: PdfParagraphAlignment) -> Self {
        PdfParagraph {
            fragments: vec![],
            bottom: None,
            left: None,
            max_width: Some(maximum_width),
            alignment,
        }
    }

    /// Returns `true` if this [PdfParagraph] contains no fragments.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    /// Returns a reference to the fragments in this paragraph.
    #[inline]
    pub fn fragments(&self) -> &[PdfParagraphFragment<'a>] {
        &self.fragments
    }

    /// Returns the bottom Y position of this paragraph, if known.
    #[inline]
    pub fn bottom(&self) -> Option<PdfPoints> {
        self.bottom
    }

    /// Returns the left X position of this paragraph, if known.
    #[inline]
    pub fn left(&self) -> Option<PdfPoints> {
        self.left
    }

    /// Returns the alignment of this paragraph.
    #[inline]
    pub fn alignment(&self) -> PdfParagraphAlignment {
        self.alignment
    }

    /// Adds a new fragment containing the given styled string to this paragraph.
    #[inline]
    pub fn push(&mut self, string: PdfStyledString<'a>) {
        if let Some(PdfParagraphFragment::StyledString(last_string)) = self.fragments.last_mut() {
            if last_string.does_match_string_styling(&string) {
                last_string.push(string.text(), " ");
            } else {
                self.fragments.push(PdfParagraphFragment::StyledString(string));
            }
        } else {
            self.fragments.push(PdfParagraphFragment::StyledString(string));
        }
    }

    /// Returns the maximum line width of this paragraph.
    #[inline]
    pub fn maximum_width(&self) -> PdfPoints {
        self.max_width.unwrap_or(PdfPoints::ZERO)
    }

    /// Sets the maximum line width of this paragraph to the given value.
    #[inline]
    pub fn set_maximum_width(&mut self, width: PdfPoints) {
        self.max_width = Some(width);
    }

    /// Returns the text contained within all text fragments in this paragraph.
    #[inline]
    pub fn text(&self) -> String {
        self.fragments
            .iter()
            .filter_map(|fragment| match fragment {
                PdfParagraphFragment::StyledString(string) => Some(string.text.as_str()),
                PdfParagraphFragment::LineBreak { .. } => Some("\n"),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Returns the text contained within all text fragments in this paragraph,
    /// separating each text fragment with the given separator.
    pub fn text_separated(&self, separator: &str) -> String {
        self.fragments
            .iter()
            .filter_map(|fragment| match fragment {
                PdfParagraphFragment::StyledString(string) => Some(string.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(separator)
    }

    /// Assembles the fragments in this paragraph into lines, taking into account the paragraph's
    /// current sizing, overflow, indent, and alignment settings. Consumes the paragraph and
    /// returns the assembled lines.
    pub fn into_lines(self) -> Vec<PdfLine<'a>> {
        let mut lines: Vec<PdfLine<'a>> = Vec::new();
        let mut current_fragments: Vec<PdfParagraphFragment<'a>> = Vec::new();
        let mut current_width = PdfPoints::ZERO;
        let mut current_bottom = self.bottom.unwrap_or(PdfPoints::ZERO);
        let mut current_left = self.left.unwrap_or(PdfPoints::ZERO);

        let effective_max_width = self.max_width.unwrap_or(PdfPoints::new(f32::MAX));

        for fragment in self.fragments {
            match fragment {
                PdfParagraphFragment::LineBreak {
                    alignment,
                    bottom: line_bottom,
                    left: line_left,
                } => {
                    if !current_fragments.is_empty() {
                        lines.push(PdfLine::new(
                            alignment,
                            current_bottom,
                            current_left,
                            current_width,
                            std::mem::take(&mut current_fragments),
                        ));
                        current_width = PdfPoints::ZERO;
                        current_bottom = line_bottom;
                        current_left = line_left;
                    }
                }
                PdfParagraphFragment::StyledString(ref styled) => {
                    let estimated_width = PdfPoints::new(styled.text().len() as f32 * styled.font_size().value * 0.5);

                    if current_width.value + estimated_width.value > effective_max_width.value
                        && !current_fragments.is_empty()
                    {
                        lines.push(PdfLine::new(
                            PdfLineAlignment::None,
                            current_bottom,
                            current_left,
                            current_width,
                            std::mem::take(&mut current_fragments),
                        ));
                        current_width = PdfPoints::ZERO;
                    }

                    current_width = PdfPoints::new(current_width.value + estimated_width.value);
                    current_fragments.push(fragment);
                }
                PdfParagraphFragment::NonTextObject(_) => {
                    current_fragments.push(fragment);
                }
            }
        }

        if !current_fragments.is_empty() {
            lines.push(PdfLine::new(
                PdfLineAlignment::None,
                current_bottom,
                current_left,
                current_width,
                current_fragments,
            ));
        }

        lines
    }
}

/// The bounds of a single positioned page object, used while assembling lines. ~keep
#[derive(Copy, Clone)]
struct ObjectBounds {
    bottom: PdfPoints,
    top: PdfPoints,
    left: PdfPoints,
    right: PdfPoints,
}

/// Tracks the previous object's position while assembling lines, used to detect line breaks
/// and to choose separators between adjacent text runs. ~keep
#[derive(Default)]
struct LastObjectInfo {
    left: Option<PdfPoints>,
    right: Option<PdfPoints>,
    bottom: Option<PdfPoints>,
    height: Option<PdfPoints>,
}

/// The fragments and position of the line currently being assembled. ~keep
struct CurrentLine<'a> {
    fragments: Vec<PdfParagraphFragment<'a>>,
    bottom: PdfPoints,
    left: PdfPoints,
    right: PdfPoints,
    alignment: PdfLineAlignment,
}

impl<'a> CurrentLine<'a> {
    fn new() -> Self {
        CurrentLine {
            fragments: Vec::new(),
            bottom: PdfPoints::ZERO,
            left: PdfPoints::ZERO,
            right: PdfPoints::ZERO,
            alignment: PdfLineAlignment::None,
        }
    }
}

/// Starts a new line when the given object begins a new visual row: either its alignment
/// differs from the current line's, or a vertical gap since the last object was detected.
/// Otherwise leaves `current` unchanged, ready for the object to be appended to it. ~keep
fn maybe_start_new_line<'a>(
    lines: &mut Vec<PdfLine<'a>>,
    current: &mut CurrentLine<'a>,
    last_object: &LastObjectInfo,
    bounds: ObjectBounds,
    paragraph_left: PdfPoints,
    paragraph_right: PdfPoints,
) {
    if last_object.left.is_some_and(|last_left| bounds.left >= last_left) {
        return;
    }

    let next_line_alignment = PdfParagraph::guess_line_alignment(
        last_object.left,
        last_object.right,
        bounds.left,
        bounds.right,
        paragraph_left,
        paragraph_right,
    );

    let last_gap_exceeds_line =
        last_object.bottom.unwrap_or(PdfPoints::ZERO) - last_object.height.unwrap_or(PdfPoints::ZERO) > bounds.top;

    if next_line_alignment == current.alignment && !last_gap_exceeds_line {
        return;
    }

    let finished_fragments = std::mem::take(&mut current.fragments);

    lines.push(PdfLine::new(
        current.alignment,
        current.bottom,
        current.left,
        bounds.right - current.left,
        finished_fragments,
    ));

    current.fragments = vec![PdfParagraphFragment::LineBreak {
        alignment: current.alignment,
        bottom: bounds.bottom,
        left: bounds.left,
    }];
    current.left = bounds.left;
    current.right = PdfPoints::ZERO;
    current.bottom = bounds.bottom;
    current.alignment = next_line_alignment;
}

/// Appends the given page object to the current line's fragments: text objects are merged into
/// the trailing [PdfStyledString] fragment when its styling matches, otherwise a new fragment is
/// pushed. Returns the updated right-hand extent of the current line. ~keep
fn append_object_to_line<'a>(
    current_line_fragments: &mut Vec<PdfParagraphFragment<'a>>,
    object: &'a PdfPageObject<'a>,
    right: PdfPoints,
    last_object_right: Option<PdfPoints>,
    current_line_right: PdfPoints,
) -> PdfPoints {
    let Some(text_object) = object.as_text_object() else {
        current_line_fragments.push(PdfParagraphFragment::NonTextObject(object.object_handle()));

        return current_line_right;
    };

    let can_merge_with_last = matches!(
        current_line_fragments.last(),
        Some(PdfParagraphFragment::StyledString(last_string)) if last_string.does_match_object_styling(text_object)
    );

    if can_merge_with_last {
        if let Some(PdfParagraphFragment::StyledString(last_string)) = current_line_fragments.last_mut() {
            let separator = separator_for_adjacent_text(text_object.bounds(), last_object_right);

            last_string.push(text_object.text(), separator);
        }
    } else {
        current_line_fragments.push(PdfParagraphFragment::StyledString(PdfStyledString::from_text_object(
            text_object,
        )));
    }

    right
}

/// Chooses the separator to use when merging adjacent text into a single [PdfStyledString]:
/// no separator when the new text starts where the previous object's right edge ended, a
/// single space otherwise (or when bounds could not be determined). ~keep
fn separator_for_adjacent_text(
    bounds: Result<PdfQuadPoints, PdfiumError>,
    last_object_right: Option<PdfPoints>,
) -> &'static str {
    let Ok(bounds) = bounds else {
        return " ";
    };

    let Some(last_object_right) = last_object_right else {
        return "";
    };

    if last_object_right > bounds.left() { "" } else { " " }
}

/// Returns true if a page object is rotated more than 10 degrees from horizontal.
///
/// Used to filter out vertical sidebar text (e.g. arXiv identifiers) that would
/// otherwise produce individual characters interleaved with body text.
fn is_significantly_rotated(object: &PdfPageObject) -> bool {
    const ROTATION_THRESHOLD_DEGREES: f32 = 10.0;
    let rotation = object.get_rotation_counter_clockwise_degrees().abs();
    let normalized = if rotation > 180.0 { 360.0 - rotation } else { rotation };
    normalized > ROTATION_THRESHOLD_DEGREES
}

#[cfg(test)]
mod tests {
    use crate::pdf::document::page::paragraph::PdfParagraph;
    use crate::prelude::*;
    use crate::utils::test::{test_bind_to_pdfium, test_fixture_path};

    #[test]
    fn test_paragraph_construction() -> Result<(), PdfiumError> {
        let pdfium = test_bind_to_pdfium();

        let document = pdfium.load_pdf_from_file(&test_fixture_path("text-test.pdf"), None)?;

        let page = document.pages().get(0)?;

        let objects = page.objects().iter().collect::<Vec<_>>();

        let paragraphs = PdfParagraph::from_objects(objects.as_slice());

        assert!(
            !paragraphs.is_empty(),
            "Expected at least one paragraph from page objects"
        );

        for paragraph in paragraphs.iter() {
            let text = paragraph.text();
            assert!(
                !text.trim().is_empty() || paragraph.is_empty(),
                "Non-empty paragraph should produce non-empty text"
            );
        }

        for paragraph in paragraphs.iter() {
            let separated = paragraph.text_separated(" ");
            let plain = paragraph.text();
            if !paragraph.is_empty() {
                assert!(
                    !separated.is_empty() || !plain.is_empty(),
                    "Text extraction should return content for non-empty paragraphs"
                );
            }
        }

        Ok(())
    }
}
