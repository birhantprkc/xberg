//! Deprecated per-font convenience constructors on [PdfFont], superseded by `PdfFonts`.
//! Split out from `font.rs` to keep that file's line count manageable; these are thin,
//! doc-hidden wrappers around [PdfFont::new_built_in] with no logic of their own. ~keep

use crate::pdf::document::PdfDocument;
use crate::pdf::document::fonts::PdfFontBuiltin;
use crate::pdf::font::PdfFont;

impl<'a> PdfFont<'a> {
    /// Creates a new [PdfFont] for the built-in "Times-Roman" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::times_roman()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::times_roman() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn times_roman(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::TimesRoman)
    }

    /// Creates a new [PdfFont] for the built-in "Times-Bold" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::times_bold()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::times_bold() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn times_bold(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::TimesBold)
    }

    /// Creates a new [PdfFont] for the built-in "Times-Italic" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::times_italic()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::times_italic() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn times_italic(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::TimesItalic)
    }

    /// Creates a new [PdfFont] for the built-in "Times-BoldItalic" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::times_bold_italic()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::times_bold_italic() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn times_bold_italic(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::TimesBoldItalic)
    }

    /// Creates a new [PdfFont] for the built-in "Helvetica" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::helvetica()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::helvetica() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn helvetica(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::Helvetica)
    }

    /// Creates a new [PdfFont] for the built-in "Helvetica-Bold" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::helvetica_bold()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::helvetica_bold() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn helvetica_bold(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::HelveticaBold)
    }

    /// Creates a new [PdfFont] for the built-in "Helvetica-Oblique" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::helvetica_oblique()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::helvetica_oblique() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn helvetica_oblique(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::HelveticaOblique)
    }

    /// Creates a new [PdfFont] for the built-in "Helvetica-BoldOblique" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::helvetica_bold_oblique()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::helvetica_bold_oblique() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn helvetica_bold_oblique(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::HelveticaBoldOblique)
    }

    /// Creates a new [PdfFont] for the built-in "Courier" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::courier()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::courier() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn courier(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::Courier)
    }

    /// Creates a new [PdfFont] for the built-in "Courier-Bold" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::courier_bold()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::courier_bold() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn courier_bold(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::CourierBold)
    }

    /// Creates a new [PdfFont] for the built-in "Courier-Oblique" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::courier_oblique()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::courier_oblique() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn courier_oblique(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::CourierOblique)
    }

    /// Creates a new [PdfFont] for the built-in "Courier-BoldOblique" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::courier_bold_oblique()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::courier_bold_oblique() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn courier_bold_oblique(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::CourierBoldOblique)
    }

    /// Creates a new [PdfFont] for the built-in "Symbol" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::symbol()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::symbol() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn symbol(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::Symbol)
    }

    /// Creates a new [PdfFont] for the built-in "ZapfDingbats" font.
    ///
    /// This function is now deprecated and will be removed in release 0.9.0.
    /// Use the `PdfFonts::zapf_dingbats()` function instead.
    #[deprecated(
        since = "0.8.1",
        note = "This function has been moved. Use the PdfFonts::zapf_dingbats() function instead."
    )]
    #[doc(hidden)]
    #[inline]
    pub fn zapf_dingbats(document: &'a PdfDocument<'a>) -> PdfFont<'a> {
        #[allow(deprecated)]
        Self::new_built_in(document, PdfFontBuiltin::ZapfDingbats)
    }
}
