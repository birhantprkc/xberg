//! Defines the [PdfPageObjectType], [PdfPageObjectBlendMode], [PdfPageObjectLineJoin], and
//! [PdfPageObjectLineCap] enums used by a single renderable [PdfPageObject].

use crate::bindgen::{
    FPDF_LINECAP_BUTT, FPDF_LINECAP_PROJECTING_SQUARE, FPDF_LINECAP_ROUND, FPDF_LINEJOIN_BEVEL, FPDF_LINEJOIN_MITER,
    FPDF_LINEJOIN_ROUND, FPDF_PAGEOBJ_FORM, FPDF_PAGEOBJ_IMAGE, FPDF_PAGEOBJ_PATH, FPDF_PAGEOBJ_SHADING,
    FPDF_PAGEOBJ_TEXT, FPDF_PAGEOBJ_UNKNOWN,
};
use crate::error::PdfiumError;
use std::os::raw::c_int;

#[cfg(doc)]
use crate::pdf::document::page::object::PdfPageObject;

/// The type of a single renderable [PdfPageObject].
///
/// Note that Pdfium does not support or recognize all PDF page object types. For instance,
/// Pdfium does not currently support or recognize all types of External Object ("XObject")
/// page object types supported by Adobe Acrobat and Foxit's commercial PDF SDK. In these cases,
/// Pdfium will return [PdfPageObjectType::Unsupported].
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Eq, Hash)]
pub enum PdfPageObjectType {
    /// Any External Object ("XObject") page object type not directly supported by Pdfium.
    Unsupported = FPDF_PAGEOBJ_UNKNOWN as isize,

    /// A page object containing renderable text.
    Text = FPDF_PAGEOBJ_TEXT as isize,

    /// A page object containing a renderable vector path.
    Path = FPDF_PAGEOBJ_PATH as isize,

    /// A page object containing a renderable bitmapped image.
    Image = FPDF_PAGEOBJ_IMAGE as isize,

    /// A page object containing a renderable geometric shape whose color is an arbitrary
    /// function of position within the shape.
    Shading = FPDF_PAGEOBJ_SHADING as isize,

    /// A page object containing a content stream that itself may consist of multiple other page
    /// objects. When this page object is rendered, it renders all its constituent page objects,
    /// effectively serving as a template or stamping object.
    ///
    /// Despite the page object name including "form", this page object type bears no relation
    /// to an interactive form containing form fields.
    XObjectForm = FPDF_PAGEOBJ_FORM as isize,
}

impl PdfPageObjectType {
    pub(crate) fn from_pdfium(value: u32) -> Result<PdfPageObjectType, PdfiumError> {
        match value {
            FPDF_PAGEOBJ_UNKNOWN => Ok(PdfPageObjectType::Unsupported),
            FPDF_PAGEOBJ_TEXT => Ok(PdfPageObjectType::Text),
            FPDF_PAGEOBJ_PATH => Ok(PdfPageObjectType::Path),
            FPDF_PAGEOBJ_IMAGE => Ok(PdfPageObjectType::Image),
            FPDF_PAGEOBJ_SHADING => Ok(PdfPageObjectType::Shading),
            FPDF_PAGEOBJ_FORM => Ok(PdfPageObjectType::XObjectForm),
            _ => Err(PdfiumError::UnknownPdfPageObjectType),
        }
    }
}

/// The method used to combine overlapping colors when painting one [PdfPageObject] on top of
/// another.
///
/// The color being newly painted is the source color; the existing color being painted onto is the
/// backdrop color.
///
/// A formal definition of these blend modes can be found in Section 7.2.4 of
/// the PDF Reference Manual, version 1.7, on page 520.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageObjectBlendMode {
    /// Selects the source color, ignoring the backdrop.
    Normal,

    /// Multiplies the backdrop and source color values. The resulting color is always at least
    /// as dark as either of the two constituent colors. Multiplying any color with black
    /// produces black; multiplying with white leaves the original color unchanged.
    /// Painting successive overlapping objects with a color other than black or white
    /// produces progressively darker colors.
    Multiply,

    /// Multiplies the complements of the backdrop and source color values, then complements
    /// the result.

    /// The result color is always at least as light as either of the two constituent colors.
    /// Screening any color with white produces white; screening with black leaves the original
    /// color unchanged. The effect is similar to projecting multiple photographic slides
    /// simultaneously onto a single screen.
    Screen,

    /// Multiplies or screens the colors, depending on the backdrop color value. Source colors
    /// overlay the backdrop while preserving its highlights and shadows. The backdrop color is
    /// not replaced but is mixed with the source color to reflect the lightness or darkness of
    /// the backdrop.
    Overlay,

    /// Selects the darker of the backdrop and source colors. The backdrop is replaced with the
    /// source where the source is darker; otherwise, it is left unchanged.
    Darken,

    /// Selects the lighter of the backdrop and source colors. The backdrop is replaced with the
    /// source where the source is lighter; otherwise, it is left unchanged.
    Lighten,

    /// Brightens the backdrop color to reflect the source color. Painting with black produces no
    /// changes.
    ColorDodge,

    /// Darkens the backdrop color to reflect the source color. Painting with white produces no
    /// change.
    ColorBurn,

    /// Multiplies or screens the colors, depending on the source color value. The effect is similar
    /// to shining a harsh spotlight on the backdrop.
    HardLight,

    /// Darkens or lightens the colors, depending on the source color value. The effect is similar
    /// to shining a diffused spotlight on the backdrop.
    SoftLight,

    /// Subtracts the darker of the two constituent colors from the lighter color.
    /// Painting with white inverts the backdrop color; painting with black produces no change.
    Difference,

    /// Produces an effect similar to that of the Difference mode but lower in contrast.
    /// Painting with white inverts the backdrop color; painting with black produces no change.
    Exclusion,

    /// Preserves the luminosity of the backdrop color while adopting the hue and saturation
    /// of the source color.
    HSLColor,

    /// Preserves the luminosity and saturation of the backdrop color while adopting the hue
    /// of the source color.
    HSLHue,

    /// Preserves the hue and saturation of the backdrop color while adopting the luminosity
    /// of the source color.
    HSLLuminosity,

    /// Preserves the luminosity and hue of the backdrop color while adopting the saturation
    /// of the source color.
    HSLSaturation,
}

impl PdfPageObjectBlendMode {
    pub(crate) fn as_pdfium(&self) -> &str {
        match self {
            PdfPageObjectBlendMode::HSLColor => "Color",
            PdfPageObjectBlendMode::ColorBurn => "ColorBurn",
            PdfPageObjectBlendMode::ColorDodge => "ColorDodge",
            PdfPageObjectBlendMode::Darken => "Darken",
            PdfPageObjectBlendMode::Difference => "Difference",
            PdfPageObjectBlendMode::Exclusion => "Exclusion",
            PdfPageObjectBlendMode::HardLight => "HardLight",
            PdfPageObjectBlendMode::HSLHue => "Hue",
            PdfPageObjectBlendMode::Lighten => "Lighten",
            PdfPageObjectBlendMode::HSLLuminosity => "Luminosity",
            PdfPageObjectBlendMode::Multiply => "Multiply",
            PdfPageObjectBlendMode::Normal => "Normal",
            PdfPageObjectBlendMode::Overlay => "Overlay",
            PdfPageObjectBlendMode::HSLSaturation => "Saturation",
            PdfPageObjectBlendMode::Screen => "Screen",
            PdfPageObjectBlendMode::SoftLight => "SoftLight",
        }
    }
}

/// The shape that should be used at the corners of stroked paths.
///
/// Join styles are significant only at points where consecutive segments of a path
/// connect at an angle; segments that meet or intersect fortuitously receive no special treatment.
///
/// A formal definition of these styles can be found in Section 4.3.2 of
/// the PDF Reference Manual, version 1.7, on page 216.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageObjectLineJoin {
    /// The outer edges of the strokes for the two path segments are extended
    /// until they meet at an angle, as in a picture frame. If the segments meet at too
    /// sharp an angle, a bevel join is used instead.
    Miter = FPDF_LINEJOIN_MITER as isize,

    /// An arc of a circle with a diameter equal to the line width is drawn
    /// around the point where the two path segments meet, connecting the outer edges of
    /// the strokes for the two segments. This pie-slice-shaped figure is filled in,
    /// producing a rounded corner.
    Round = FPDF_LINEJOIN_ROUND as isize,

    /// The two path segments are finished with butt caps and the resulting notch
    /// beyond the ends of the segments is filled with a triangle.
    Bevel = FPDF_LINEJOIN_BEVEL as isize,
}

impl PdfPageObjectLineJoin {
    pub(crate) fn from_pdfium(value: c_int) -> Option<Self> {
        match value as u32 {
            FPDF_LINEJOIN_MITER => Some(Self::Miter),
            FPDF_LINEJOIN_ROUND => Some(Self::Round),
            FPDF_LINEJOIN_BEVEL => Some(Self::Bevel),
            _ => None,
        }
    }

    pub(crate) fn as_pdfium(&self) -> u32 {
        match self {
            PdfPageObjectLineJoin::Miter => FPDF_LINEJOIN_MITER,
            PdfPageObjectLineJoin::Round => FPDF_LINEJOIN_ROUND,
            PdfPageObjectLineJoin::Bevel => FPDF_LINEJOIN_BEVEL,
        }
    }
}

/// The shape that should be used at the ends of open stroked paths.
///
/// A formal definition of these styles can be found in Section 4.3.2 of
/// the PDF Reference Manual, version 1.7, on page 216.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageObjectLineCap {
    /// The stroke is squared off at the endpoint of the path. There is no
    /// projection beyond the end of the path.
    Butt = FPDF_LINECAP_BUTT as isize,

    /// A semicircular arc with a diameter equal to the line width is
    /// drawn around the endpoint and filled in.
    Round = FPDF_LINECAP_ROUND as isize,

    /// The stroke continues beyond the endpoint of the path
    /// for a distance equal to half the line width and is squared off.
    Square = FPDF_LINECAP_PROJECTING_SQUARE as isize,
}

impl PdfPageObjectLineCap {
    pub(crate) fn from_pdfium(value: c_int) -> Option<Self> {
        match value as u32 {
            FPDF_LINECAP_BUTT => Some(Self::Butt),
            FPDF_LINECAP_ROUND => Some(Self::Round),
            FPDF_LINECAP_PROJECTING_SQUARE => Some(Self::Square),
            _ => None,
        }
    }

    pub(crate) fn as_pdfium(&self) -> u32 {
        match self {
            PdfPageObjectLineCap::Butt => FPDF_LINECAP_BUTT,
            PdfPageObjectLineCap::Round => FPDF_LINECAP_ROUND,
            PdfPageObjectLineCap::Square => FPDF_LINECAP_PROJECTING_SQUARE,
        }
    }
}
