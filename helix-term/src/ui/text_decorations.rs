use std::cmp::Ordering;

use helix_core::doc_formatter::FormattedGrapheme;
use helix_core::Position;
use helix_view::editor::CursorCache;

use crate::ui::document::{LinePos, TextRenderer};

pub use diagnostics::InlineDiagnostics;

pub mod blame;
mod diagnostics;

/// Decorations are the primary mechanism for extending the text rendering.
///
/// Any on-screen element which is anchored to the rendered text in some form
/// should be implemented using this trait. Translating char positions to
/// on-screen positions can be expensive and should not be done manually in the
/// ui loop. Instead such translations are automatically performed on the fly
/// while the text is being rendered. The results are provided to this trait by
/// the rendering infrastructure.
///
/// To reserve space for virtual text lines (which is then filled by this trait) emit appropriate
/// [`LineAnnotation`](helix_core::text_annotations::LineAnnotation)s in [`helix_view::View::text_annotations`]
pub trait Decoration {
    /// Called **before** a **visual** line is rendered. A visual line does not
    /// necessarily correspond to a single line in a document as soft wrapping can
    /// spread a single document line across multiple visual lines.
    ///
    /// This function is called before text is rendered as any decorations should
    /// never overlap the document text. That means that setting the forground color
    /// here is (essentially) useless as the text color is overwritten by the
    /// rendered text. This _of course_ doesn't apply when rendering inside virtual lines
    /// below the line reserved by `LineAnnotation`s as no text will be rendered here.
    fn decorate_line(&mut self, _renderer: &mut TextRenderer, _pos: LinePos) {}

    /// Called **after** a **visual** line is rendered. A visual line does not
    /// necessarily correspond to a single line in a document as soft wrapping can
    /// spread a single document line across multiple visual lines.
    ///
    /// This function is called after text is rendered so that decorations can collect
    /// horizontal positions on the line (see [`Decoration::decorate_grapheme`]) first and
    /// use those positions while rendering virtual text. That means that setting the foreground
    /// color here is (essentially) useless as the text color is overwritten by the
    /// rendered text. This _of course_ doesn't apply when rendering inside virtual lines
    /// below the line reserved by `LineAnnotation`s. e as no text will be rendered here.
    /// **Note**: To avoid overlapping decorations in the virtual lines, each decoration
    /// must return the number of virtual text lines it has taken up. Each `Decoration` receives
    /// an offset `virt_off` based on these return values where it can render virtual text:
    ///
    /// That means that a `render_line` implementation that returns `X` can render virtual text
    /// in the following area:
    ///
    /// ```ignore
    /// let start = inner.y + pos.virtual_line + virt_off;
    /// start .. start + X
    /// ````
    fn render_virt_lines(
        &mut self,
        _renderer: &mut TextRenderer,
        _pos: LinePos,
        _virt_off: Position,
    ) -> Position {
        Position::new(0, 0)
    }

    fn reset_pos(&mut self, _pos: usize) -> usize {
        usize::MAX
    }

    fn skip_concealed_anchor(&mut self, conceal_end_char_idx: usize) -> usize {
        self.reset_pos(conceal_end_char_idx)
    }

    /// This function is called **before** the grapheme at `char_idx` is rendered.
    ///
    /// # Returns
    ///
    /// The char idx of the next grapheme that  this function should be called for
    fn decorate_grapheme(
        &mut self,
        _renderer: &mut TextRenderer,
        _grapheme: &FormattedGrapheme,
    ) -> usize {
        usize::MAX
    }
}

impl<F: FnMut(&mut TextRenderer, LinePos)> Decoration for F {
    fn decorate_line(&mut self, renderer: &mut TextRenderer, pos: LinePos) {
        self(renderer, pos);
    }
}

#[derive(Default)]
pub struct DecorationManager<'a> {
    decorations: Vec<(Box<dyn Decoration + 'a>, usize)>,
}

impl<'a> DecorationManager<'a> {
    pub fn add_decoration(&mut self, decoration: impl Decoration + 'a) {
        self.decorations.push((Box::new(decoration), 0));
    }

    pub fn prepare_for_rendering(&mut self, first_visible_char: usize) {
        for (decoration, next_position) in &mut self.decorations {
            *next_position = decoration.reset_pos(first_visible_char)
        }
    }

    pub fn decorate_grapheme(&mut self, renderer: &mut TextRenderer, grapheme: &FormattedGrapheme) {
        for (decoration, hook_char_idx) in &mut self.decorations {
            loop {
                match (*hook_char_idx).cmp(&grapheme.char_idx) {
                    // this grapheme has been concealed or we are at the first grapheme
                    Ordering::Less => {
                        *hook_char_idx = decoration.skip_concealed_anchor(grapheme.char_idx)
                    }
                    Ordering::Equal => {
                        *hook_char_idx = decoration.decorate_grapheme(renderer, grapheme)
                    }
                    Ordering::Greater => break,
                }
            }
        }
    }

    pub fn decorate_line(&mut self, renderer: &mut TextRenderer, pos: LinePos) {
        for (decoration, _) in &mut self.decorations {
            decoration.decorate_line(renderer, pos);
        }
    }

    pub fn render_virtual_lines(
        &mut self,
        renderer: &mut TextRenderer,
        pos: LinePos,
        line_width: usize,
    ) {
        let mut virt_off = Position::new(1, line_width); // start at 1 so the line is never overwritten
        for (decoration, _) in &mut self.decorations {
            virt_off += decoration.render_virt_lines(renderer, pos, virt_off);
        }
    }
}

/// Cursor rendering is done externally so all the cursor decoration
/// does is save the position of primary cursor
pub struct Cursor<'a> {
    pub cache: &'a CursorCache,
    pub primary_cursor: usize,
}
impl Decoration for Cursor<'_> {
    fn reset_pos(&mut self, pos: usize) -> usize {
        if pos <= self.primary_cursor {
            self.primary_cursor
        } else {
            usize::MAX
        }
    }

    fn decorate_grapheme(
        &mut self,
        renderer: &mut TextRenderer,
        grapheme: &FormattedGrapheme,
    ) -> usize {
        if renderer.column_in_bounds(grapheme.visual_pos.col, grapheme.width())
            && renderer.offset.row < grapheme.visual_pos.row
        {
            let position = grapheme.visual_pos - renderer.offset;
            self.cache.set(Some(position));
        }
        usize::MAX
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use helix_core::{
        config::default_lang_loader,
        diagnostic::{Diagnostic, DiagnosticProvider, LanguageServerId, Range, Severity},
        doc_formatter::TextFormat,
        text_annotations::{InlineAnnotation, TextAnnotations},
        Rope,
    };
    use helix_view::{
        annotations::diagnostics::{DiagnosticFilter, InlineDiagnosticsConfig},
        editor::{Config, GutterConfig},
        graphics::Rect,
        Document, DocumentId, Theme, View,
    };
    use tui::buffer::Buffer as Surface;

    use super::*;
    use crate::ui::document::render_text;

    fn doc(text: &str) -> Document {
        Document::from(
            Rope::from(text),
            None,
            Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            Arc::new(ArcSwap::new(Arc::new(default_lang_loader()))),
        )
    }

    fn wrap_format(viewport: Rect) -> TextFormat {
        TextFormat {
            soft_wrap: true,
            viewport_width: viewport.width,
            wrap_indicator: "↩".into(),
            ..Default::default()
        }
    }

    fn render(
        doc: &Document,
        text_fmt: &TextFormat,
        annotations: &TextAnnotations,
        viewport: Rect,
        decorations: DecorationManager,
    ) -> Surface {
        let theme: Theme = toml::from_str("").unwrap();
        let mut surface = Surface::empty(viewport);
        let mut renderer =
            TextRenderer::new(&mut surface, doc, &theme, Position::new(0, 0), viewport);
        render_text(
            &mut renderer,
            doc.text().slice(..),
            0,
            text_fmt,
            annotations,
            None,
            Vec::new(),
            &theme,
            decorations,
        );
        surface
    }

    fn diagnostic_connector(
        mut doc: Document,
        start: usize,
        text_fmt: &TextFormat,
        annotations: &TextAnnotations,
        viewport: Rect,
    ) -> Option<(u16, u16)> {
        let end = doc.text().len_chars();
        doc.replace_diagnostics(
            [Diagnostic {
                range: Range { start, end },
                ends_at_word: false,
                starts_at_word: false,
                zero_width: false,
                line: 0,
                message: "msg".into(),
                severity: Some(Severity::Warning),
                code: None,
                provider: DiagnosticProvider::Lsp {
                    server_id: LanguageServerId::default(),
                    identifier: None,
                },
                tags: Vec::new(),
                source: None,
                data: None,
            }],
            &[],
            None,
        );
        let config = InlineDiagnosticsConfig {
            cursor_line: DiagnosticFilter::Enable(Severity::Hint),
            other_lines: DiagnosticFilter::Enable(Severity::Hint),
            min_diagnostic_width: 1,
            prefix_len: 1,
            max_wrap: 20,
            max_diagnostics: 10,
        };
        let theme: Theme = toml::from_str("").unwrap();
        let mut decorations = DecorationManager::default();
        decorations.add_decoration(InlineDiagnostics::new(
            &doc,
            &theme,
            0,
            config,
            DiagnosticFilter::Disable,
        ));
        let surface = render(&doc, text_fmt, annotations, viewport, decorations);

        (viewport.y..viewport.bottom())
            .flat_map(|y| (viewport.x..viewport.right()).map(move |x| (x, y)))
            .find(|&(x, y)| {
                surface
                    .get(x, y)
                    .is_some_and(|cell| cell.symbol.as_str() == "└")
            })
    }

    #[test]
    fn cursor_lands_on_char_after_wrap_indicator() {
        let doc = doc("aaaa bbbb cccc dddd");
        let viewport = Rect::new(0, 0, 10, 4);
        let cache = CursorCache::default();
        let mut decorations = DecorationManager::default();
        // char 10 is the 'c' that starts the first wrapped row
        decorations.add_decoration(Cursor {
            cache: &cache,
            primary_cursor: 10,
        });
        render(
            &doc,
            &wrap_format(viewport),
            &TextAnnotations::default(),
            viewport,
            decorations,
        );

        let view = View::new(DocumentId::default(), GutterConfig::default());
        assert_eq!(cache.get(&view, &doc), Some(Position::new(1, 1)));
    }

    #[test]
    fn diagnostic_anchors_on_char_after_wrap_indicator() {
        let viewport = Rect::new(0, 0, 10, 6);
        // char 10 is the 'c' at col 1 of the wrapped row, after the indicator at col 0
        let connector = diagnostic_connector(
            doc("aaaa bbbb cccc"),
            10,
            &wrap_format(viewport),
            &TextAnnotations::default(),
            viewport,
        );
        assert_eq!(connector, Some((1, 2)));
    }

    #[test]
    fn diagnostic_anchors_on_char_after_inlay_hint() {
        let viewport = Rect::new(0, 0, 20, 4);
        let hints = [InlineAnnotation::new(3, "XY: ")];
        let mut annotations = TextAnnotations::default();
        annotations.add_inline_annotations(&hints, None);
        // the hint takes cols 3 to 6, so the 'c' at char 3 sits at col 7
        let connector = diagnostic_connector(
            doc("ab cd"),
            3,
            &TextFormat::default(),
            &annotations,
            viewport,
        );
        assert_eq!(connector, Some((7, 1)));
    }
}
