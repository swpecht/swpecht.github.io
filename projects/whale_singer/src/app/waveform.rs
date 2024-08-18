use itertools::Itertools;
use ratatui::prelude::*;
use ratatui::widgets::Widget;

/// Show a waveform of audio samples using the RMS function
/// https://manual.audacityteam.org/man/glossary.html#rms
#[derive(Debug, Default)]
pub struct WaveformWidget {
    samples: Vec<f32>,
}

impl WaveformWidget {
    pub fn set_samples(&mut self, samples: Vec<f32>) {
        self.samples = samples;
    }
}

/// https://manual.audacityteam.org/man/glossary.html#rms
fn rms(samples: &[f32]) -> f32 {
    let sqr_sum = samples.iter().fold(0.0, |sqr_sum, s| {
        let sample = *s;
        sqr_sum + sample * sample
    });
    (sqr_sum / samples.len() as f32).sqrt()
}

impl Widget for &mut WaveformWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.samples.is_empty() {
            return;
        }

        let chunk_size = self.samples.len() / area.width as usize;
        let mut processed_data = self
            .samples
            .clone()
            .into_iter()
            .chunks(chunk_size)
            .into_iter()
            .map(|x| rms(&x.collect_vec()))
            .collect_vec();
        let max = *processed_data
            .iter()
            .max_by(|a, b| a.partial_cmp(b).expect("tried to compare nan"))
            .unwrap();

        processed_data
            .iter_mut()
            .for_each(|x| *x = *x / max * area.height as f32 * 2.);

        for (xi, x) in (area.left()..area.right()).enumerate() {
            for (yi, y) in (area.top()..area.bottom()).rev().enumerate() {
                let bg = if processed_data[xi] as usize >= 2 * yi {
                    Color::Blue
                } else {
                    Color::Black
                };

                let fg = if processed_data[xi] as usize > 2 * yi {
                    Color::Blue
                } else {
                    Color::Black
                };

                // render a half block character for each row of pixels with the foreground color
                // set to the color of the pixel and the background color set to the color of the
                // pixel below it
                buf.get_mut(x, y).set_char('▀').set_fg(fg).set_bg(bg);
            }
        }
    }
}
