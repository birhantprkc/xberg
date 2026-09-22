//! Defines the [PdfPageAnnotation] struct, exposing functionality related to a single annotation.

pub mod attachment_points;
pub mod circle;
mod common;
pub mod free_text;
pub mod highlight;
pub mod ink;
pub mod link;
pub mod objects;
pub(crate) mod private;
pub mod square;
pub mod squiggly;
pub mod stamp;
pub mod strikeout;
pub mod text;
pub mod underline;
pub mod unsupported;

pub use common::PdfPageAnnotationCommon;

use crate::bindgen::{
    FPDF_ANNOT_CARET, FPDF_ANNOT_CIRCLE, FPDF_ANNOT_FILEATTACHMENT, FPDF_ANNOT_FREETEXT, FPDF_ANNOT_HIGHLIGHT,
    FPDF_ANNOT_INK, FPDF_ANNOT_LINE, FPDF_ANNOT_LINK, FPDF_ANNOT_MOVIE, FPDF_ANNOT_POLYGON, FPDF_ANNOT_POLYLINE,
    FPDF_ANNOT_POPUP, FPDF_ANNOT_PRINTERMARK, FPDF_ANNOT_REDACT, FPDF_ANNOT_RICHMEDIA, FPDF_ANNOT_SCREEN,
    FPDF_ANNOT_SOUND, FPDF_ANNOT_SQUARE, FPDF_ANNOT_SQUIGGLY, FPDF_ANNOT_STAMP, FPDF_ANNOT_STRIKEOUT, FPDF_ANNOT_TEXT,
    FPDF_ANNOT_THREED, FPDF_ANNOT_TRAPNET, FPDF_ANNOT_UNDERLINE, FPDF_ANNOT_UNKNOWN, FPDF_ANNOT_WATERMARK,
    FPDF_ANNOT_WIDGET, FPDF_ANNOT_XFAWIDGET, FPDF_ANNOTATION, FPDF_ANNOTATION_SUBTYPE, FPDF_DOCUMENT, FPDF_FORMHANDLE,
    FPDF_PAGE,
};
use crate::bindings::PdfiumLibraryBindings;
use crate::error::PdfiumError;
use crate::pdf::document::page::annotation::attachment_points::PdfPageAnnotationAttachmentPoints;
use crate::pdf::document::page::annotation::circle::PdfPageCircleAnnotation;
use crate::pdf::document::page::annotation::free_text::PdfPageFreeTextAnnotation;
use crate::pdf::document::page::annotation::highlight::PdfPageHighlightAnnotation;
use crate::pdf::document::page::annotation::ink::PdfPageInkAnnotation;
use crate::pdf::document::page::annotation::link::PdfPageLinkAnnotation;
use crate::pdf::document::page::annotation::objects::PdfPageAnnotationObjects;
use crate::pdf::document::page::annotation::private::internal::PdfPageAnnotationPrivate;
use crate::pdf::document::page::annotation::square::PdfPageSquareAnnotation;
use crate::pdf::document::page::annotation::squiggly::PdfPageSquigglyAnnotation;
use crate::pdf::document::page::annotation::stamp::PdfPageStampAnnotation;
use crate::pdf::document::page::annotation::strikeout::PdfPageStrikeoutAnnotation;
use crate::pdf::document::page::annotation::text::PdfPageTextAnnotation;
use crate::pdf::document::page::annotation::underline::PdfPageUnderlineAnnotation;
use crate::pdf::document::page::annotation::unsupported::PdfPageUnsupportedAnnotation;
use crate::pdf::document::page::object::ownership::PdfPageObjectOwnership;

#[cfg(doc)]
use crate::pdf::document::page::PdfPage;

/// The type of a single [PdfPageAnnotation], as defined in table 8.20 of the PDF Reference,
/// version 1.7, on page 615.
///
/// Not all PDF annotation types are supported by Pdfium. For example, Pdfium does not
/// currently support embedded sound or movie file annotations, embedded 3D animations, or
/// annotations containing embedded file attachments.
///
/// Pdfium currently supports creating, editing, and rendering the following types of annotations:
///
/// * [PdfPageAnnotationType::Circle]
/// * [PdfPageAnnotationType::FreeText]
/// * [PdfPageAnnotationType::Highlight]
/// * [PdfPageAnnotationType::Ink]
/// * [PdfPageAnnotationType::Link]
/// * [PdfPageAnnotationType::Popup]
/// * [PdfPageAnnotationType::Redacted]
/// * [PdfPageAnnotationType::Square]
/// * [PdfPageAnnotationType::Squiggly]
/// * [PdfPageAnnotationType::Stamp]
/// * [PdfPageAnnotationType::Strikeout]
/// * [PdfPageAnnotationType::Text]
/// * [PdfPageAnnotationType::Underline]
/// * [PdfPageAnnotationType::Widget]
/// * [PdfPageAnnotationType::XfaWidget]
///
/// Note that a `FreeText` annotation is rendered directly on the page, whereas a `Text` annotation
/// floats over the page inside its own enclosed area. Adobe often uses the term "sticky note"
/// in reference to `Text` annotations to distinguish them from `FreeText` annotations.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub enum PdfPageAnnotationType {
    Unknown = FPDF_ANNOT_UNKNOWN as isize,
    Text = FPDF_ANNOT_TEXT as isize,
    Link = FPDF_ANNOT_LINK as isize,
    FreeText = FPDF_ANNOT_FREETEXT as isize,
    Line = FPDF_ANNOT_LINE as isize,
    Square = FPDF_ANNOT_SQUARE as isize,
    Circle = FPDF_ANNOT_CIRCLE as isize,
    Polygon = FPDF_ANNOT_POLYGON as isize,
    Polyline = FPDF_ANNOT_POLYLINE as isize,
    Highlight = FPDF_ANNOT_HIGHLIGHT as isize,
    Underline = FPDF_ANNOT_UNDERLINE as isize,
    Squiggly = FPDF_ANNOT_SQUIGGLY as isize,
    Strikeout = FPDF_ANNOT_STRIKEOUT as isize,
    Stamp = FPDF_ANNOT_STAMP as isize,
    Caret = FPDF_ANNOT_CARET as isize,
    Ink = FPDF_ANNOT_INK as isize,
    Popup = FPDF_ANNOT_POPUP as isize,
    FileAttachment = FPDF_ANNOT_FILEATTACHMENT as isize,
    Sound = FPDF_ANNOT_SOUND as isize,
    Movie = FPDF_ANNOT_MOVIE as isize,
    Widget = FPDF_ANNOT_WIDGET as isize,
    Screen = FPDF_ANNOT_SCREEN as isize,
    PrinterMark = FPDF_ANNOT_PRINTERMARK as isize,
    TrapNet = FPDF_ANNOT_TRAPNET as isize,
    Watermark = FPDF_ANNOT_WATERMARK as isize,
    ThreeD = FPDF_ANNOT_THREED as isize,
    RichMedia = FPDF_ANNOT_RICHMEDIA as isize,
    XfaWidget = FPDF_ANNOT_XFAWIDGET as isize,
    Redacted = FPDF_ANNOT_REDACT as isize,
}

impl PdfPageAnnotationType {
    pub(crate) fn from_pdfium(value: FPDF_ANNOTATION_SUBTYPE) -> Result<PdfPageAnnotationType, PdfiumError> {
        match value as u32 {
            FPDF_ANNOT_UNKNOWN => Ok(PdfPageAnnotationType::Unknown),
            FPDF_ANNOT_TEXT => Ok(PdfPageAnnotationType::Text),
            FPDF_ANNOT_LINK => Ok(PdfPageAnnotationType::Link),
            FPDF_ANNOT_FREETEXT => Ok(PdfPageAnnotationType::FreeText),
            FPDF_ANNOT_LINE => Ok(PdfPageAnnotationType::Line),
            FPDF_ANNOT_SQUARE => Ok(PdfPageAnnotationType::Square),
            FPDF_ANNOT_CIRCLE => Ok(PdfPageAnnotationType::Circle),
            FPDF_ANNOT_POLYGON => Ok(PdfPageAnnotationType::Polygon),
            FPDF_ANNOT_POLYLINE => Ok(PdfPageAnnotationType::Polyline),
            FPDF_ANNOT_HIGHLIGHT => Ok(PdfPageAnnotationType::Highlight),
            FPDF_ANNOT_UNDERLINE => Ok(PdfPageAnnotationType::Underline),
            FPDF_ANNOT_SQUIGGLY => Ok(PdfPageAnnotationType::Squiggly),
            FPDF_ANNOT_STRIKEOUT => Ok(PdfPageAnnotationType::Strikeout),
            FPDF_ANNOT_STAMP => Ok(PdfPageAnnotationType::Stamp),
            FPDF_ANNOT_CARET => Ok(PdfPageAnnotationType::Caret),
            FPDF_ANNOT_INK => Ok(PdfPageAnnotationType::Ink),
            FPDF_ANNOT_POPUP => Ok(PdfPageAnnotationType::Popup),
            FPDF_ANNOT_FILEATTACHMENT => Ok(PdfPageAnnotationType::FileAttachment),
            FPDF_ANNOT_SOUND => Ok(PdfPageAnnotationType::Sound),
            FPDF_ANNOT_MOVIE => Ok(PdfPageAnnotationType::Movie),
            FPDF_ANNOT_WIDGET => Ok(PdfPageAnnotationType::Widget),
            FPDF_ANNOT_SCREEN => Ok(PdfPageAnnotationType::Screen),
            FPDF_ANNOT_PRINTERMARK => Ok(PdfPageAnnotationType::PrinterMark),
            FPDF_ANNOT_TRAPNET => Ok(PdfPageAnnotationType::TrapNet),
            FPDF_ANNOT_WATERMARK => Ok(PdfPageAnnotationType::Watermark),
            FPDF_ANNOT_THREED => Ok(PdfPageAnnotationType::ThreeD),
            FPDF_ANNOT_RICHMEDIA => Ok(PdfPageAnnotationType::RichMedia),
            FPDF_ANNOT_XFAWIDGET => Ok(PdfPageAnnotationType::XfaWidget),
            FPDF_ANNOT_REDACT => Ok(PdfPageAnnotationType::Redacted),
            _ => Err(PdfiumError::UnknownPdfAnnotationType),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn as_pdfium(&self) -> FPDF_ANNOTATION_SUBTYPE {
        (match self {
            PdfPageAnnotationType::Unknown => FPDF_ANNOT_UNKNOWN,
            PdfPageAnnotationType::Text => FPDF_ANNOT_TEXT,
            PdfPageAnnotationType::Link => FPDF_ANNOT_LINK,
            PdfPageAnnotationType::FreeText => FPDF_ANNOT_FREETEXT,
            PdfPageAnnotationType::Line => FPDF_ANNOT_LINE,
            PdfPageAnnotationType::Square => FPDF_ANNOT_SQUARE,
            PdfPageAnnotationType::Circle => FPDF_ANNOT_CIRCLE,
            PdfPageAnnotationType::Polygon => FPDF_ANNOT_POLYGON,
            PdfPageAnnotationType::Polyline => FPDF_ANNOT_POLYLINE,
            PdfPageAnnotationType::Highlight => FPDF_ANNOT_HIGHLIGHT,
            PdfPageAnnotationType::Underline => FPDF_ANNOT_UNDERLINE,
            PdfPageAnnotationType::Squiggly => FPDF_ANNOT_SQUIGGLY,
            PdfPageAnnotationType::Strikeout => FPDF_ANNOT_STRIKEOUT,
            PdfPageAnnotationType::Stamp => FPDF_ANNOT_STAMP,
            PdfPageAnnotationType::Caret => FPDF_ANNOT_CARET,
            PdfPageAnnotationType::Ink => FPDF_ANNOT_INK,
            PdfPageAnnotationType::Popup => FPDF_ANNOT_POPUP,
            PdfPageAnnotationType::FileAttachment => FPDF_ANNOT_FILEATTACHMENT,
            PdfPageAnnotationType::Sound => FPDF_ANNOT_SOUND,
            PdfPageAnnotationType::Movie => FPDF_ANNOT_MOVIE,
            PdfPageAnnotationType::Widget => FPDF_ANNOT_WIDGET,
            PdfPageAnnotationType::Screen => FPDF_ANNOT_SCREEN,
            PdfPageAnnotationType::PrinterMark => FPDF_ANNOT_PRINTERMARK,
            PdfPageAnnotationType::TrapNet => FPDF_ANNOT_TRAPNET,
            PdfPageAnnotationType::Watermark => FPDF_ANNOT_WATERMARK,
            PdfPageAnnotationType::ThreeD => FPDF_ANNOT_THREED,
            PdfPageAnnotationType::RichMedia => FPDF_ANNOT_RICHMEDIA,
            PdfPageAnnotationType::XfaWidget => FPDF_ANNOT_XFAWIDGET,
            PdfPageAnnotationType::Redacted => FPDF_ANNOT_REDACT,
        }) as FPDF_ANNOTATION_SUBTYPE
    }
}

/// A single user annotation on a [PdfPage].
pub enum PdfPageAnnotation<'a> {
    Circle(PdfPageCircleAnnotation<'a>),
    FreeText(PdfPageFreeTextAnnotation<'a>),
    Highlight(PdfPageHighlightAnnotation<'a>),
    Ink(PdfPageInkAnnotation<'a>),
    Link(PdfPageLinkAnnotation<'a>),
    Square(PdfPageSquareAnnotation<'a>),
    Squiggly(PdfPageSquigglyAnnotation<'a>),
    Stamp(PdfPageStampAnnotation<'a>),
    Strikeout(PdfPageStrikeoutAnnotation<'a>),
    Text(PdfPageTextAnnotation<'a>),
    Underline(PdfPageUnderlineAnnotation<'a>),

    /// Common properties shared by all [PdfPageAnnotation] types can still be accessed for
    /// annotations not supported by Pdfium, but annotation-specific functionality
    /// will be unavailable.
    Unsupported(PdfPageUnsupportedAnnotation<'a>),
}

impl<'a> PdfPageAnnotation<'a> {
    pub(crate) fn from_pdfium(
        document_handle: FPDF_DOCUMENT,
        page_handle: FPDF_PAGE,
        annotation_handle: FPDF_ANNOTATION,
        _form_handle: Option<FPDF_FORMHANDLE>,
        bindings: &'a dyn PdfiumLibraryBindings,
    ) -> Self {
        let annotation_type = PdfPageAnnotationType::from_pdfium(bindings.FPDFAnnot_GetSubtype(annotation_handle))
            .unwrap_or(PdfPageAnnotationType::Unknown);

        // Every supported annotation type is constructed from the same four arguments; this
        // local macro avoids repeating that call eleven times over. ~keep
        macro_rules! variant {
            ($ctor:ty) => {
                <$ctor>::from_pdfium(document_handle, page_handle, annotation_handle, bindings)
            };
        }

        match annotation_type {
            PdfPageAnnotationType::Circle => PdfPageAnnotation::Circle(variant!(PdfPageCircleAnnotation)),
            PdfPageAnnotationType::FreeText => PdfPageAnnotation::FreeText(variant!(PdfPageFreeTextAnnotation)),
            PdfPageAnnotationType::Highlight => PdfPageAnnotation::Highlight(variant!(PdfPageHighlightAnnotation)),
            PdfPageAnnotationType::Ink => PdfPageAnnotation::Ink(variant!(PdfPageInkAnnotation)),
            PdfPageAnnotationType::Link => PdfPageAnnotation::Link(variant!(PdfPageLinkAnnotation)),
            PdfPageAnnotationType::Square => PdfPageAnnotation::Square(variant!(PdfPageSquareAnnotation)),
            PdfPageAnnotationType::Squiggly => PdfPageAnnotation::Squiggly(variant!(PdfPageSquigglyAnnotation)),
            PdfPageAnnotationType::Stamp => PdfPageAnnotation::Stamp(variant!(PdfPageStampAnnotation)),
            PdfPageAnnotationType::Strikeout => PdfPageAnnotation::Strikeout(variant!(PdfPageStrikeoutAnnotation)),
            PdfPageAnnotationType::Text => PdfPageAnnotation::Text(variant!(PdfPageTextAnnotation)),
            PdfPageAnnotationType::Underline => PdfPageAnnotation::Underline(variant!(PdfPageUnderlineAnnotation)),
            _ => PdfPageAnnotation::Unsupported(PdfPageUnsupportedAnnotation::from_pdfium(
                document_handle,
                page_handle,
                annotation_handle,
                annotation_type,
                bindings,
            )),
        }
    }

    #[inline]
    pub(crate) fn unwrap_as_trait(&self) -> &dyn PdfPageAnnotationPrivate<'a> {
        match self {
            PdfPageAnnotation::Circle(annotation) => annotation,
            PdfPageAnnotation::FreeText(annotation) => annotation,
            PdfPageAnnotation::Highlight(annotation) => annotation,
            PdfPageAnnotation::Ink(annotation) => annotation,
            PdfPageAnnotation::Link(annotation) => annotation,
            PdfPageAnnotation::Square(annotation) => annotation,
            PdfPageAnnotation::Squiggly(annotation) => annotation,
            PdfPageAnnotation::Stamp(annotation) => annotation,
            PdfPageAnnotation::Strikeout(annotation) => annotation,
            PdfPageAnnotation::Text(annotation) => annotation,
            PdfPageAnnotation::Underline(annotation) => annotation,
            PdfPageAnnotation::Unsupported(annotation) => annotation,
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn unwrap_as_trait_mut(&mut self) -> &mut dyn PdfPageAnnotationPrivate<'a> {
        match self {
            PdfPageAnnotation::Circle(annotation) => annotation,
            PdfPageAnnotation::FreeText(annotation) => annotation,
            PdfPageAnnotation::Highlight(annotation) => annotation,
            PdfPageAnnotation::Ink(annotation) => annotation,
            PdfPageAnnotation::Link(annotation) => annotation,
            PdfPageAnnotation::Square(annotation) => annotation,
            PdfPageAnnotation::Squiggly(annotation) => annotation,
            PdfPageAnnotation::Stamp(annotation) => annotation,
            PdfPageAnnotation::Strikeout(annotation) => annotation,
            PdfPageAnnotation::Text(annotation) => annotation,
            PdfPageAnnotation::Underline(annotation) => annotation,
            PdfPageAnnotation::Unsupported(annotation) => annotation,
        }
    }

    /// The type of this [PdfPageAnnotation].
    ///
    /// Not all PDF annotation types are supported by Pdfium. For example, Pdfium does not
    /// currently support embedded sound or movie file annotations, embedded 3D animations, or
    /// annotations containing embedded file attachments.
    ///
    /// Pdfium currently supports creating, editing, and rendering the following types of annotations:
    ///
    /// * [PdfPageAnnotationType::Circle]
    /// * [PdfPageAnnotationType::FreeText]
    /// * [PdfPageAnnotationType::Highlight]
    /// * [PdfPageAnnotationType::Ink]
    /// * [PdfPageAnnotationType::Link]
    /// * [PdfPageAnnotationType::Popup]
    /// * [PdfPageAnnotationType::Redacted]
    /// * [PdfPageAnnotationType::Square]
    /// * [PdfPageAnnotationType::Squiggly]
    /// * [PdfPageAnnotationType::Stamp]
    /// * [PdfPageAnnotationType::Strikeout]
    /// * [PdfPageAnnotationType::Text]
    /// * [PdfPageAnnotationType::Underline]
    /// * [PdfPageAnnotationType::Widget]
    /// * [PdfPageAnnotationType::XfaWidget]
    #[inline]
    pub fn annotation_type(&self) -> PdfPageAnnotationType {
        match self {
            PdfPageAnnotation::Circle(_) => PdfPageAnnotationType::Circle,
            PdfPageAnnotation::FreeText(_) => PdfPageAnnotationType::FreeText,
            PdfPageAnnotation::Highlight(_) => PdfPageAnnotationType::Highlight,
            PdfPageAnnotation::Ink(_) => PdfPageAnnotationType::Ink,
            PdfPageAnnotation::Link(_) => PdfPageAnnotationType::Link,
            PdfPageAnnotation::Square(_) => PdfPageAnnotationType::Square,
            PdfPageAnnotation::Squiggly(_) => PdfPageAnnotationType::Squiggly,
            PdfPageAnnotation::Stamp(_) => PdfPageAnnotationType::Stamp,
            PdfPageAnnotation::Strikeout(_) => PdfPageAnnotationType::Strikeout,
            PdfPageAnnotation::Text(_) => PdfPageAnnotationType::Text,
            PdfPageAnnotation::Underline(_) => PdfPageAnnotationType::Underline,
            PdfPageAnnotation::Unsupported(annotation) => annotation.get_type(),
        }
    }

    /// Returns `true` if Pdfium supports creating, editing, and rendering this type of
    /// [PdfPageAnnotation].
    ///
    /// Not all PDF annotation types are supported by Pdfium. For example, Pdfium does not
    /// currently support embedded sound or movie file annotations, embedded 3D animations, or
    /// annotations containing embedded file attachments.
    ///
    /// Pdfium currently supports creating, editing, and rendering the following types of annotations:
    ///
    /// * [PdfPageAnnotationType::Circle]
    /// * [PdfPageAnnotationType::FreeText]
    /// * [PdfPageAnnotationType::Highlight]
    /// * [PdfPageAnnotationType::Ink]
    /// * [PdfPageAnnotationType::Link]
    /// * [PdfPageAnnotationType::Popup]
    /// * [PdfPageAnnotationType::Redacted]
    /// * [PdfPageAnnotationType::Square]
    /// * [PdfPageAnnotationType::Squiggly]
    /// * [PdfPageAnnotationType::Stamp]
    /// * [PdfPageAnnotationType::Strikeout]
    /// * [PdfPageAnnotationType::Text]
    /// * [PdfPageAnnotationType::Underline]
    /// * [PdfPageAnnotationType::Widget]
    /// * [PdfPageAnnotationType::XfaWidget]
    #[inline]
    pub fn is_supported(&self) -> bool {
        !self.is_unsupported()
    }

    /// Returns `true` if Pdfium does _not_ support creating, editing, and rendering this type of
    /// [PdfPageAnnotation].
    ///
    /// Not all PDF annotation types are supported by Pdfium. For example, Pdfium does not
    /// currently support embedded sound or movie file annotations, embedded 3D animations, or
    /// annotations containing embedded file attachments.
    ///
    /// Pdfium currently supports creating, editing, and rendering the following types of annotations:
    ///
    /// * [PdfPageAnnotationType::Circle]
    /// * [PdfPageAnnotationType::FreeText]
    /// * [PdfPageAnnotationType::Highlight]
    /// * [PdfPageAnnotationType::Ink]
    /// * [PdfPageAnnotationType::Link]
    /// * [PdfPageAnnotationType::Popup]
    /// * [PdfPageAnnotationType::Redacted]
    /// * [PdfPageAnnotationType::Square]
    /// * [PdfPageAnnotationType::Squiggly]
    /// * [PdfPageAnnotationType::Stamp]
    /// * [PdfPageAnnotationType::Strikeout]
    /// * [PdfPageAnnotationType::Text]
    /// * [PdfPageAnnotationType::Underline]
    /// * [PdfPageAnnotationType::Widget]
    /// * [PdfPageAnnotationType::XfaWidget]
    #[inline]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, PdfPageAnnotation::Unsupported(_))
    }

    /// Returns an immutable reference to the underlying [PdfPageCircleAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Circle].
    #[inline]
    pub fn as_circle_annotation(&self) -> Option<&PdfPageCircleAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Circle(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageCircleAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Circle].
    #[inline]
    pub fn as_circle_annotation_mut(&mut self) -> Option<&mut PdfPageCircleAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Circle(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageFreeTextAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::FreeText].
    #[inline]
    pub fn as_free_text_annotation(&self) -> Option<&PdfPageFreeTextAnnotation<'_>> {
        match self {
            PdfPageAnnotation::FreeText(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageFreeTextAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::FreeText].
    #[inline]
    pub fn as_free_text_annotation_mut(&mut self) -> Option<&mut PdfPageFreeTextAnnotation<'a>> {
        match self {
            PdfPageAnnotation::FreeText(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageHighlightAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Highlight].
    #[inline]
    pub fn as_highlight_annotation(&self) -> Option<&PdfPageHighlightAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Highlight(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageHighlightAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Highlight].
    #[inline]
    pub fn as_highlight_annotation_mut(&mut self) -> Option<&mut PdfPageHighlightAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Highlight(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageInkAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Ink].
    #[inline]
    pub fn as_ink_annotation(&self) -> Option<&PdfPageInkAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Ink(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageInkAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Ink].
    #[inline]
    pub fn as_ink_annotation_mut(&mut self) -> Option<&mut PdfPageInkAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Ink(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageLinkAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Link].
    #[inline]
    pub fn as_link_annotation(&self) -> Option<&PdfPageLinkAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Link(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageLinkAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Link].
    #[inline]
    pub fn as_link_annotation_mut(&mut self) -> Option<&mut PdfPageLinkAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Link(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageSquareAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Square].
    #[inline]
    pub fn as_square_annotation(&self) -> Option<&PdfPageSquareAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Square(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageSquareAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Square].
    #[inline]
    pub fn as_square_annotation_mut(&mut self) -> Option<&mut PdfPageSquareAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Square(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageSquigglyAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Squiggly].
    #[inline]
    pub fn as_squiggly_annotation(&self) -> Option<&PdfPageSquigglyAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Squiggly(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageSquigglyAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Squiggly].
    #[inline]
    pub fn as_squiggly_annotation_mut(&mut self) -> Option<&mut PdfPageSquigglyAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Squiggly(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageStampAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Stamp].
    #[inline]
    pub fn as_stamp_annotation(&self) -> Option<&PdfPageStampAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Stamp(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageStampAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Stamp].
    #[inline]
    pub fn as_stamp_annotation_mut(&mut self) -> Option<&mut PdfPageStampAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Stamp(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageStrikeoutAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Strikeout].
    #[inline]
    pub fn as_strikeout_annotation(&self) -> Option<&PdfPageStrikeoutAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Strikeout(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageStrikeoutAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Strikeout].
    #[inline]
    pub fn as_strikeout_annotation_mut(&mut self) -> Option<&mut PdfPageStrikeoutAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Strikeout(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageTextAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Text].
    #[inline]
    pub fn as_text_annotation(&self) -> Option<&PdfPageTextAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Text(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageTextAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Text].
    #[inline]
    pub fn as_text_annotation_mut(&mut self) -> Option<&mut PdfPageTextAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Text(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns an immutable reference to the underlying [PdfPageUnderlineAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Underline].
    #[inline]
    pub fn as_underline_annotation(&self) -> Option<&PdfPageUnderlineAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Underline(annotation) => Some(annotation),
            _ => None,
        }
    }

    /// Returns a mutable reference to the underlying [PdfPageUnderlineAnnotation]
    /// for this [PdfPageAnnotation], if this annotation has an annotation type of
    /// [PdfPageAnnotationType::Underline].
    #[inline]
    pub fn as_underline_annotation_mut(&mut self) -> Option<&mut PdfPageUnderlineAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Underline(annotation) => Some(annotation),
            _ => None,
        }
    }
}

impl<'a> PdfPageAnnotationPrivate<'a> for PdfPageAnnotation<'a> {
    #[inline]
    fn handle(&self) -> FPDF_ANNOTATION {
        self.unwrap_as_trait().handle()
    }

    #[inline]
    fn bindings(&self) -> &dyn PdfiumLibraryBindings {
        self.unwrap_as_trait().bindings()
    }

    #[inline]
    fn ownership(&self) -> &PdfPageObjectOwnership {
        self.unwrap_as_trait().ownership()
    }

    #[inline]
    fn objects_impl(&self) -> &PdfPageAnnotationObjects<'_> {
        self.unwrap_as_trait().objects_impl()
    }

    #[inline]
    fn attachment_points_impl(&self) -> &PdfPageAnnotationAttachmentPoints<'_> {
        self.unwrap_as_trait().attachment_points_impl()
    }
}

impl<'a> Drop for PdfPageAnnotation<'a> {
    /// Closes this [PdfPageAnnotation], releasing held memory.
    #[inline]
    fn drop(&mut self) {
        self.bindings().FPDFPage_CloseAnnot(self.handle());
    }
}
