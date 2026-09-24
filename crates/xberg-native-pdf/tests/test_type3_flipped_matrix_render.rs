//! Regression test for GH#1780: the page renderer draws nothing for text set
//! in a Type 3 font whose `/FontMatrix` flips the y axis
//! (`[0.01 0 0 -0.01 0 0]`) and whose glyphs are filled paths drawn with
//! `d1`. The fixture is the reporter's `repro-type3-flipped-matrix.pdf`
//! (sha256 `ed7e2b6ab19d7f0f86ed22601d1b80739332289a3d474566e88411d1626e8935`,
//! verified against the value quoted in the issue), pinned unmodified as a
//! regression fixture per `partner-corpus-handling` / `test-corpus`
//! conventions (a synthetic, non-partner file with no confidentiality
//! concerns, well under the size that would need bucket-hosting).
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]

use xberg_native_pdf::PdfDocument;
use xberg_native_pdf::rendering::{ImageFormat, PageRenderer, RenderOptions, RenderedImage};

use tracing_subscriber::layer::SubscriberExt;

const FIXTURE: &[u8] = include_bytes!("fixtures/regressions/type3/gh1780-flipped-matrix.pdf");

/// Mirrors `WarnCapture` in `test_render_output_intent.rs` — duplicated
/// because the integration test crate can't reach in-crate test types.
#[derive(Clone, Default)]
struct WarnCapture {
    buf: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl<S> tracing_subscriber::Layer<S> for WarnCapture
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        if *event.metadata().level() > tracing::Level::WARN {
            return;
        }
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        self.buf.lock().unwrap().push(visitor.0);
    }
}

struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

/// The page holds four lines of Type 3 text set in black on a white
/// background. A renderer that paints the glyphs must leave non-white
/// (dark) pixels somewhere in the glyph region; a renderer that silently
/// drops every glyph description leaves the whole page white.
fn count_dark_pixels(img: &RenderedImage) -> usize {
    img.data
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] < 128 && px[1] < 128 && px[2] < 128)
        .count()
}

/// Horizontal extent (max_x - min_x, in pixels) of every dark pixel in the
/// image. The longest line, "General Fund 48,210,500 49,850,300" (35
/// characters at 16pt), must advance across a wide span of the page —
/// a per-glyph advance bug that ignores the font's `/FontMatrix` scale
/// collapses every glyph onto (almost) the same x position instead,
/// producing a narrow, unreadable ink blob rather than spread-out text.
fn dark_pixel_x_span(img: &RenderedImage) -> u32 {
    let mut min_x = u32::MAX;
    let mut max_x = 0u32;
    for (i, px) in img.data.as_chunks::<4>().0.iter().enumerate() {
        if px[0] < 128 && px[1] < 128 && px[2] < 128 {
            let x = (i as u32) % img.width;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
        }
    }
    max_x.saturating_sub(min_x)
}

#[test]
fn type3_flipped_font_matrix_glyphs_paint_ink_on_the_page() {
    let capture = WarnCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());

    let doc = PdfDocument::from_bytes(FIXTURE.to_vec()).expect("parse GH#1780 fixture");

    let mut opts = RenderOptions::with_dpi(150);
    opts.format = ImageFormat::RawRgba8;
    let mut renderer = PageRenderer::new(opts);

    let img = tracing::subscriber::with_default(subscriber, || {
        renderer.render_page(&doc, 0).expect("render GH#1780 page 0")
    });

    let dark = count_dark_pixels(&img);
    let x_span = dark_pixel_x_span(&img);

    let records: Vec<String> = capture.buf.lock().unwrap().clone();
    eprintln!("warnings captured during render: {records:#?}");
    eprintln!(
        "dark pixels painted: {dark}, horizontal span: {x_span}px (image width {})",
        img.width
    );

    assert!(
        dark > 500,
        "expected the four lines of Type 3 text (FontMatrix [0.01 0 0 -0.01 0 0], \
         d1 filled-path glyphs) to paint visible ink on the {}x{} page, found only \
         {dark} dark pixels — the renderer is drawing nothing for this font, \
         reproducing GH#1780",
        img.width,
        img.height
    );

    // The longest line is 35 characters at 16pt; at 150dpi it must span a
    // wide fraction of the page width. A per-glyph advance that ignores
    // /FontMatrix's scale collapses every character onto nearly the same
    // x position (GH#1780), producing an unreadable smear.
    assert!(
        x_span > 300,
        "expected Type 3 glyphs to advance across a wide horizontal span \
         (35-character line at 16pt), found only {x_span}px of horizontal \
         spread in dark pixels across a {}px-wide page — glyphs are \
         collapsing onto (almost) the same x position, reproducing GH#1780's \
         unreadable render",
        img.width
    );
}
