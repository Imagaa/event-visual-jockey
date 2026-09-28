//! Output configuration: which screen, what it shows, and how the picture is sliced onto it.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputSource {
    #[default]
    Composition,
    /// A single layer (before it is blended), e.g. a side screen showing only the speaker slides.
    Layer(usize),
}

/// Part of the source (`input`) drawn into part of the output (`output`); rects are x, y, w, h in 0..1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slice {
    pub name: String,
    pub input: [f32; 4],
    pub output: [f32; 4],
}

impl Default for Slice {
    fn default() -> Self {
        Slice::full()
    }
}

fn clamp_rect(r: [f32; 4]) -> [f32; 4] {
    let x = r[0].clamp(0.0, 1.0);
    let y = r[1].clamp(0.0, 1.0);
    [x, y, r[2].clamp(0.0, 1.0 - x), r[3].clamp(0.0, 1.0 - y)]
}

impl Slice {
    pub fn full() -> Slice {
        Slice { name: "Full".into(), input: [0.0, 0.0, 1.0, 1.0], output: [0.0, 0.0, 1.0, 1.0] }
    }

    pub fn clamped(mut self) -> Slice {
        self.input = clamp_rect(self.input);
        self.output = clamp_rect(self.output);
        self
    }

    /// `n` vertical strips, each mapped to the same place on the output.
    pub fn columns(n: usize) -> Vec<Slice> {
        let w = 1.0 / n.max(1) as f32;
        (0..n.max(1))
            .map(|i| {
                let r = [i as f32 * w, 0.0, w, 1.0];
                Slice { name: format!("Column {}", i + 1), input: r, output: r }
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub name: String,
    /// Monitor name as Windows reports it; None = a window on the operator screen.
    pub monitor: Option<String>,
    pub source: OutputSource,
    pub slices: Vec<Slice>,
    /// Shows a numbered test pattern instead of the source (setup / identify).
    pub test_pattern: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig { name: "Main".into(), monitor: None, source: OutputSource::Composition, slices: vec![Slice::full()], test_pattern: false }
    }
}
