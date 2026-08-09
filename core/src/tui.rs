use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{io, time::Duration, sync::mpsc::Receiver};
use cortex::Stats;

pub enum TuiMsg {
    Update {
        processed: usize,
        total: usize,
        current_file: String,
        speed_mb: f64,
        eta_secs: f64,
    },
    Done(Stats),
    Error(String),
}

pub fn run_tui(rx: Receiver<TuiMsg>, is_compress: bool, input_name: String) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut processed = 0;
    let mut total = 1;
    let mut speed = 0.0;
    let mut eta = 0.0;
    let mut current_file = String::new();
    let mut done = false;
    let mut error = None;
    let mut final_stats: Option<Stats> = None;

    loop {
        terminal.draw(|f| {
            let size = f.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Min(0),
                    ]
                    .as_ref(),
                )
                .split(size);

            let title = Paragraph::new(Span::styled(
                " ⚛ C O R T E X  -  N E X T  G E N  C O M P R E S S O R ⚛ ",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::DarkGray)));

            f.render_widget(title, chunks[0]);

            let percent = if total > 0 { (processed as f64 / total as f64).clamp(0.0, 1.0) } else { 0.0 };
            
            let action_text = if is_compress { "Compressing" } else { "Extracting" };
            let gauge = Gauge::default()
                .block(Block::default().title(format!(" {} : {} ", action_text, input_name)).borders(Borders::ALL))
                .gauge_style(Style::default().fg(Color::Green).bg(Color::Black).add_modifier(Modifier::ITALIC))
                .ratio(percent)
                .label(format!("{:.1}%", percent * 100.0));
            f.render_widget(gauge, chunks[1]);

            let stats_text = format!(
                " Processed: {} / {} | Speed: {:.1} MB/s | ETA: {:.0}s ",
                format_bytes(processed), format_bytes(total), speed, eta
            );
            
            let stats_para = Paragraph::new(stats_text)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            
            f.render_widget(stats_para, chunks[2]);
            
            let log_text = if let Some(ref e) = error {
                format!("ERROR: {}", e)
            } else if done {
                if let Some(ref s) = final_stats {
                    if is_compress {
                        let ratio = 100.0 - (s.output_size as f64 / s.input_size.max(1) as f64 * 100.0);
                        format!("✅ SUCCESS!\n\nInput: {}\nOutput: {}\nSaved: {:.2}%\nTime: {:.2}s\nChunks: {}\n\nPress ENTER or Q to exit.", 
                                format_bytes(s.input_size), format_bytes(s.output_size), ratio, s.elapsed.as_secs_f64(), s.chunks)
                    } else {
                        format!("✅ SUCCESS!\n\nDecompressed: {}\nTime: {:.2}s\nChunks: {}\n\nPress ENTER or Q to exit.", 
                                format_bytes(s.output_size), s.elapsed.as_secs_f64(), s.chunks)
                    }
                } else {
                    "✅ DONE!\nPress ENTER or Q to exit.".to_string()
                }
            } else {
                format!("> {}", current_file)
            };

            let log_para = Paragraph::new(log_text)
                .style(if error.is_some() { Style::default().fg(Color::Red) } else if done { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) })
                .block(Block::default().title(" Status ").borders(Borders::ALL));
                
            f.render_widget(log_para, chunks[3]);
        })?;

        if let Ok(msg) = rx.recv_timeout(Duration::from_millis(16)) {
            match msg {
                TuiMsg::Update { processed: p, total: t, current_file: f, speed_mb: s, eta_secs: e } => {
                    processed = p;
                    total = t;
                    current_file = f;
                    speed = s;
                    eta = e;
                }
                TuiMsg::Done(stats) => {
                    done = true;
                    final_stats = Some(stats);
                    processed = total; // pin to 100%
                }
                TuiMsg::Error(err) => {
                    error = Some(err);
                    done = true;
                }
            }
        }

        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
                if done && key.code == KeyCode::Enter {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.2} KB", bytes as f64 / 1024.0) }
    else if bytes < 1024 * 1024 * 1024 { format!("{:.2} MB", bytes as f64 / 1048576.0) }
    else { format!("{:.2} GB", bytes as f64 / 1073741824.0) }
}
