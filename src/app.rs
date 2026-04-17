use anyhow::Result;
use chrono::Local;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use std::io::Stdout;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

use crate::modules::{self, OutputLine, MODULES};
use crate::sound::SoundEngine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Input,
    Running,
}

pub struct App {
    pub mode:            Mode,
    pub selected:        usize,
    pub input:           String,
    pub output:          Vec<OutputLine>,
    pub scroll:          u16,
    pub auto_scroll:     bool,
    pub tick:            u64,
    pub status:          String,
    pub viewport_height: u16,
    pub list_state:      ListState,
    pub sound:           SoundEngine,

    rx: Option<mpsc::Receiver<OutputLine>>,
}

impl App {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let output = vec![
            OutputLine::Dim("━".repeat(50)),
            OutputLine::Bright("SYSTEM INITIALIZATION COMPLETE.".into()),
            OutputLine::Normal("MU/TH/UR 6000 ONLINE.".into()),
            OutputLine::Normal("WEYLAND-YUTANI CORP — NETWORK RECONNAISSANCE SUITE.".into()),
            OutputLine::Bright("7 MODULES LOADED. ALL SYSTEMS NOMINAL.".into()),
            OutputLine::Dim("━".repeat(50)),
            OutputLine::Normal("SELECT MODULE AND ENTER TARGET TO BEGIN.".into()),
            OutputLine::Bright("AWAITING INSTRUCTION.".into()),
        ];

        let sound = SoundEngine::new();
        sound.boot(); // play startup chime

        Self {
            mode: Mode::Browse,
            selected: 0,
            input: String::new(),
            output,
            scroll: 0,
            auto_scroll: true,
            tick: 0,
            status: "AWAITING INSTRUCTION.".into(),
            viewport_height: 20,
            list_state,
            sound,
            rx: None,
        }
    }

    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let mut events     = EventStream::new();
        let mut tick_timer = time::interval(Duration::from_millis(80));

        loop {
            // ── Drain scan output (up to 64 lines per frame) ─────────────────
            let mut pending: Vec<OutputLine> = Vec::new();
            let mut scan_done = false;
            if let Some(rx) = &mut self.rx {
                for _ in 0..64 {
                    match rx.try_recv() {
                        Ok(OutputLine::Done) => { scan_done = true; break; }
                        Ok(line)             => pending.push(line),
                        Err(_)               => break,
                    }
                }
            }
            // Process collected lines (no borrow of self.rx)
            for line in pending {
                match &line {
                    OutputLine::Error(_) => self.sound.error(),
                    _                    => self.sound.output_tick(),
                }
                self.output.push(line);
                self.tick_scroll();
            }
            if scan_done {
                self.rx     = None;
                self.mode   = Mode::Browse;
                self.status = "ANALYSIS COMPLETE. AWAITING INSTRUCTION.".into();
                self.output.push(OutputLine::Dim("━".repeat(50)));
                self.sound.scan_complete();
            }

            // ── Render ───────────────────────────────────────────────────────
            terminal.draw(|frame| {
                crate::ui::draw(frame, self);
            })?;

            // ── Event / tick ─────────────────────────────────────────────────
            tokio::select! {
                _ = tick_timer.tick() => {
                    self.tick = self.tick.wrapping_add(1);
                }
                maybe = events.next() => {
                    match maybe {
                        Some(Ok(event)) => {
                            if self.handle_event(event)? {
                                return Ok(());
                            }
                        }
                        Some(Err(_)) => {}
                        None => return Ok(()),
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: Event) -> Result<bool> {
        if let Event::Key(key) = event {
            return self.handle_key(key);
        }
        Ok(false)
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        match self.mode {
            Mode::Browse  => self.key_browse(key),
            Mode::Input   => self.key_input(key),
            Mode::Running => self.key_running(key),
        }
    }

    // ── Browse mode ───────────────────────────────────────────────────────────

    fn key_browse(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),

            KeyCode::Char('m') | KeyCode::Char('M') => {
                self.sound.toggle_mute();
                let state = if self.sound.is_muted() { "MUTED" } else { "UNMUTED" };
                self.status = format!("AUDIO {}.", state);
            }

            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.list_state.select(Some(self.selected));
                    self.sound.select();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + 1 < MODULES.len() {
                    self.selected += 1;
                    self.list_state.select(Some(self.selected));
                    self.sound.select();
                }
            }
            KeyCode::Enter | KeyCode::Tab => {
                self.mode   = Mode::Input;
                self.status = format!(
                    "INPUT MODE  {}  —  ENTER TARGET, THEN PRESS [ENTER] TO EXECUTE.",
                    MODULES[self.selected].name
                );
                self.sound.input_mode();
            }
            KeyCode::PageUp => {
                self.auto_scroll = false;
                self.scroll = self.scroll.saturating_sub(self.viewport_height);
            }
            KeyCode::PageDown => {
                let max = self.max_scroll();
                if self.scroll >= max {
                    self.auto_scroll = true;
                } else {
                    self.scroll = (self.scroll + self.viewport_height).min(max);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    // ── Input mode ────────────────────────────────────────────────────────────

    fn key_input(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.mode   = Mode::Browse;
                self.status = "AWAITING INSTRUCTION.".into();
                self.sound.cancel();
            }
            KeyCode::Tab => {
                self.mode   = Mode::Browse;
                self.status = "AWAITING INSTRUCTION.".into();
                self.sound.cancel();
            }
            KeyCode::Enter => {
                self.start_scan()?;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                self.sound.cancel();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            KeyCode::Char('m') | KeyCode::Char('M')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                // ctrl+m falls through to char handler below — prevent silent toggle
                self.input.push('m');
                self.sound.keypress();
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.sound.keypress();
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.sound.keypress();
            }
            _ => {}
        }
        Ok(false)
    }

    // ── Running mode ──────────────────────────────────────────────────────────

    fn key_running(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                self.sound.toggle_mute();
            }
            KeyCode::PageUp => {
                self.auto_scroll = false;
                self.scroll = self.scroll.saturating_sub(self.viewport_height);
            }
            KeyCode::PageDown => {
                let max = self.max_scroll();
                self.scroll = (self.scroll + self.viewport_height).min(max);
                if self.scroll >= max {
                    self.auto_scroll = true;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    // ── Scan launch ───────────────────────────────────────────────────────────

    fn start_scan(&mut self) -> Result<()> {
        let target = self.input.trim().to_string();
        if target.is_empty() {
            self.output.push(OutputLine::Error(
                "NO TARGET SPECIFIED. ENTER TARGET AND RETRY.".into(),
            ));
            self.sound.error();
            self.auto_scroll = true;
            self.update_scroll();
            return Ok(());
        }

        let module_idx  = self.selected;
        let module_name = MODULES[module_idx].name;

        let ts = Local::now().format("%Y.%m.%d %H:%M:%S").to_string();
        self.output.push(OutputLine::Dim(format!(
            "━━━  {}  ━━━  {}  ━━━",
            ts, module_name
        )));
        self.auto_scroll = true;
        self.update_scroll();

        let (tx, rx) = mpsc::channel::<OutputLine>(4096);
        self.rx     = Some(rx);
        self.mode   = Mode::Running;
        self.status = format!("EXECUTING: {}  —  [PGUP/PGDN] SCROLL  [M] MUTE  [Q] ABORT", module_name);
        self.input.clear();
        self.sound.scan_start();

        tokio::spawn(async move {
            if let Err(e) = modules::run_module(module_idx, target, tx.clone()).await {
                tx.send(OutputLine::Error(format!(
                    "MODULE FAULT: {}",
                    e.to_string().to_uppercase()
                )))
                .await
                .ok();
                tx.send(OutputLine::Done).await.ok();
            }
        });

        Ok(())
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn max_scroll(&self) -> u16 {
        self.output
            .len()
            .saturating_sub(self.viewport_height as usize) as u16
    }

    fn update_scroll(&mut self) {
        if self.auto_scroll {
            self.scroll = self.max_scroll();
        }
    }

    fn tick_scroll(&mut self) {
        if self.auto_scroll {
            self.scroll = self
                .output
                .len()
                .saturating_sub(self.viewport_height as usize) as u16;
        }
    }
}
