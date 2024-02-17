use anyhow::Ok;
use log::info;
use sonogram::{ColourGradient, ColourTheme, FrequencyScale, SpecOptionsBuilder};
use std::{
    fs,
    io::stdout,
    panic,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget};

use color_eyre::{config::HookBuilder, eyre};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};

use ratatui::{prelude::*, widgets::Block};

pub fn run_app() -> color_eyre::Result<()> {
    install_error_hooks()?;

    // Set max_log_level to Trace
    tui_logger::init_logger(log::LevelFilter::Trace).unwrap();

    // Set default level for unknown targets to Trace
    tui_logger::set_default_level(log::LevelFilter::Trace);

    let terminal = init_terminal()?;
    App::default().run(terminal).unwrap();
    restore_terminal()?;
    color_eyre::Result::Ok(())
}

use crate::{
    decode::extract_samples,
    encode::{save_wav, SAMPLE_RATE},
    optimization::{add_best_atom, AtomSearchResult},
};
#[derive(Debug)]
struct App {
    /// The current state of the app (running or quit)
    state: AppState,

    target_spectogram: SpectogramWidget,
    current_spectogram: SpectogramWidget,

    /// Channles for managing work
    tx: Sender<Vec<f32>>,
    rx: Receiver<Vec<f32>>,
}

impl Default for App {
    fn default() -> Self {
        let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = mpsc::channel();
        Self {
            state: Default::default(),
            target_spectogram: Default::default(),
            current_spectogram: Default::default(),
            tx,
            rx,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
enum AppState {
    /// The app is running
    #[default]
    Running,

    /// The user has requested the app to quit
    Quit,
}

/// A widget that displays the full range of RGB colors that can be displayed in the terminal.
///
/// This widget is animated and will change colors over time.
#[derive(Debug, Default)]
struct SpectogramWidget {
    /// The colors to render - should be double the height of the area as we render two rows of
    /// pixels for each row of the widget using the half block character. This is computed any time
    /// the size of the widget changes.
    colors: Vec<Vec<Color>>,

    samples: Vec<f32>,

    /// flag to determine if need to re-draw
    is_dirty: bool,
}

impl App {
    /// Run the app
    ///
    /// This is the main event loop for the app.
    pub fn run(mut self, mut terminal: Terminal<impl Backend>) -> anyhow::Result<()> {
        let src = std::fs::File::open("im_different_sample.wav").expect("failed to open media");
        // let src = std::fs::File::open("happy_birthday.mp3").expect("failed to open media");
        let mut target_samples = extract_samples(src).unwrap();
        target_samples.truncate(SAMPLE_RATE * 8);
        self.target_spectogram.set_samples(target_samples.clone());

        let thread_tx = self.tx.clone();
        let target = target_samples;
        thread::spawn(move || {
            // let paths = fs::read_dir("../../../piano-mp3/piano-mp3/").unwrap();

            let atom_files = ["A4", "B4", "C4", "D4", "E4", "F4", "G4"];
            let mut atoms = Vec::new();
            for atom_id in atom_files {
                let path_name = format!("../../../piano-mp3/piano-mp3/{}.mp3", atom_id);
                // let src =
                //     std::fs::File::open(atom_file.unwrap().path()).expect("failed to open media");
                let src = std::fs::File::open(path_name).expect("failed to open media");
                let mut key_samples = extract_samples(src).unwrap();
                // only the the middle 1 second
                key_samples = key_samples[SAMPLE_RATE * 3 / 4..SAMPLE_RATE * 5 / 4].to_vec();
                atoms.push(key_samples);
            }

            info!("loaded {} atoms", atoms.len());

            let mut output = vec![0.0; target.len()];

            for _ in 0..20 {
                match add_best_atom(&mut output, &target, &atoms).unwrap() {
                    AtomSearchResult::NoImprovement => {
                        info!("failed to find improvement");
                        break;
                    }
                    AtomSearchResult::Found {
                        start,
                        atom_index,
                        old_error,
                        new_error,
                    } => {
                        save_wav("output.wav", &output).unwrap();
                        info!(
                            "found improvement with atom: {} at start: {}, old error: {}, new error: {}",
                            atom_index, start, old_error, new_error
                        );
                        thread_tx.send(output.clone()).unwrap();
                    }
                }
            }
        });

        while self.is_running() {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.size()))?;
            self.handle_events()?;
            self.handle_samples()?;
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        matches!(self.state, AppState::Running)
    }

    /// Handle any events that have occurred since the last time the app was rendered.
    ///
    /// Currently, this only handles the q key to quit the app.
    fn handle_events(&mut self) -> anyhow::Result<()> {
        // Ensure that the app only blocks for a period that allows the app to render at
        // approximately 60 FPS (this doesn't account for the time to render the frame, and will
        // also update the app immediately any time an event occurs)
        let timeout = Duration::from_secs_f32(1.0 / 60.0);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    self.state = AppState::Quit;
                };
            }
        }
        Ok(())
    }

    fn handle_samples(&mut self) -> anyhow::Result<()> {
        if let std::result::Result::Ok(samples) = self.rx.try_recv() {
            self.current_spectogram.set_samples(samples);
        }

        Ok(())
    }
}

/// Implement the Widget trait for &mut App so that it can be rendered
///
/// This is implemented on a mutable reference so that the app can update its state while it is
/// being rendered. This allows the fps widget to update the fps calculation and the colors widget
/// to update the colors to render.
impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use Constraint::*;
        let [top, spectograms, logs] = Layout::vertical([Length(1), Min(0), Max(20)]).areas(area);
        Text::from("whale singer. Press q to quit")
            .centered()
            .render(top, buf);
        let [target, current] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(spectograms);

        let [target_label, target_spec] =
            Layout::horizontal([Constraint::Max(10), Constraint::Min(0)]).areas(target);
        Text::from("target").centered().render(target_label, buf);
        self.target_spectogram.render(target_spec, buf);

        let [cur_label, cur_spec] =
            Layout::horizontal([Constraint::Max(10), Constraint::Min(0)]).areas(current);
        Text::from("current").centered().render(cur_label, buf);
        self.current_spectogram.render(cur_spec, buf);

        TuiLoggerWidget::default()
            .block(Block::bordered().title("Logs"))
            .output_separator('|')
            .output_timestamp(Some("%F %H:%M:%S%.3f".to_string()))
            .output_level(Some(TuiLoggerLevelOutput::Long))
            .output_target(false)
            .output_file(false)
            .output_line(false)
            .style(Style::default().fg(Color::White))
            .render(logs, buf);
    }
}

/// Widget impl for ColorsWidget
///
/// This is implemented on a mutable reference so that we can update the frame count and store a
/// cached version of the colors to render instead of recalculating them every frame.
impl Widget for &mut SpectogramWidget {
    /// Render the widget
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.setup_colors(area);
        let colors = &self.colors;

        // self.setup_colors(area);
        // let colors = &self.colors;
        for (xi, x) in (area.left()..area.right()).enumerate() {
            for (yi, y) in (area.top()..area.bottom()).enumerate() {
                // render a half block character for each row of pixels with the foreground color
                // set to the color of the pixel and the background color set to the color of the
                // pixel below it
                let fg = colors[yi * 2][xi];
                let bg = colors[yi * 2 + 1][xi];
                buf.get_mut(x, y).set_char('▀').set_fg(fg).set_bg(bg);
            }
        }
    }
}

impl SpectogramWidget {
    /// Setup the colors to render.
    ///
    /// This is called once per frame to setup the colors to render. It caches the colors so that
    /// they don't need to be recalculated every frame.
    fn setup_colors(&mut self, size: Rect) {
        let Rect { width, height, .. } = size;
        // double the height because each screen row has two rows of half block pixels
        let height = height as usize * 2;
        let width = width as usize;
        // only update the colors if the size has changed since the last time we rendered
        if self.colors.len() == height && self.colors[0].len() == width && !self.is_dirty {
            return;
        }
        self.colors = calculate_spectogram(self.samples.clone(), width, height);
        self.is_dirty = false;
    }

    fn set_samples(&mut self, samples: Vec<f32>) {
        self.samples = samples;
        self.is_dirty = true;
    }
}

/// Install color_eyre panic and error hooks
///
/// The hooks restore the terminal to a usable state before printing the error message.
fn install_error_hooks() -> color_eyre::Result<()> {
    let (panic, error) = HookBuilder::default().into_hooks();
    let panic = panic.into_panic_hook();
    let error = error.into_eyre_hook();
    eyre::set_hook(Box::new(move |e| {
        let _ = restore_terminal();
        error(e)
    }))?;
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        panic(info)
    }));
    color_eyre::Result::Ok(())
}

fn init_terminal() -> color_eyre::Result<Terminal<impl Backend>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;
    terminal.hide_cursor()?;
    color_eyre::Result::Ok(terminal)
}

fn restore_terminal() -> color_eyre::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    color_eyre::Result::Ok(())
}

fn calculate_spectogram(samples: Vec<f32>, width: usize, height: usize) -> Vec<Vec<Color>> {
    let mut colors = vec![vec![Color::Black; width]; height];

    if samples.is_empty() {
        return colors;
    }

    let mut spectrograph = SpecOptionsBuilder::new(1024)
        .load_data_from_memory_f32(samples, SAMPLE_RATE as u32)
        .build()
        .unwrap();
    // Compute the spectrogram giving the number of bins and the window overlap.
    let mut spectrograph = spectrograph.compute();

    // Specify a colour gradient to use (note you can create custom ones)
    let mut gradient = ColourGradient::create(ColourTheme::Default);

    let rgba = spectrograph.to_rgba_in_memory(FrequencyScale::Linear, &mut gradient, width, height);

    for x in 0..width {
        (0..height).for_each(|y| {
            let idx = 4 * (x + y * width);
            let c = Color::Rgb(rgba[idx], rgba[idx + 1], rgba[idx + 2]);
            colors[y][x] = c;
        });
    }

    colors
}
