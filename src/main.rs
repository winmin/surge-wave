use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::stream::{self, StreamExt};
use m3u8_rs::Playlist;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use url::Url;

// Surge 配色方案
const COLOR_NEON_PURPLE: Color = Color::Magenta;
const COLOR_NEON_PINK: Color = Color::LightMagenta;
const COLOR_NEON_CYAN: Color = Color::Cyan;
const COLOR_COMPLETED: Color = Color::Green;
const COLOR_FAILED: Color = Color::Red;
const COLOR_GRAY: Color = Color::DarkGray;

/// 展开路径中的 ~ 符号
fn expand_path(path: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(path).as_ref())
}

#[derive(Parser, Debug)]
#[command(author, version, about = "M3U8下载器 - Surge四象限布局")]
struct Args {
    /// M3U8链接URL
    url: String,

    /// 输出文件名（不含扩展名）
    #[arg(short, long)]
    output: String,

    /// 下载目录
    #[arg(short, long, default_value = "downloads")]
    dir: String,

    /// 并发下载数
    #[arg(short, long, default_value = "10")]
    concurrent: usize,
}

#[derive(Clone)]
struct ActivityItem {
    name: String,
    status: ActivityStatus,
}

#[derive(Clone, PartialEq)]
enum ActivityStatus {
    Success,
    Failed,
    Downloading,
}

struct DownloadStats {
    total_segments: usize,
    downloaded_segments: usize,
    failed_segments: usize,
    downloaded_bytes: u64,
    start_time: Instant,
    current_speed: f64,
    speed_history: VecDeque<f64>,
    chunk_states: Vec<ChunkState>,
    activity_log: VecDeque<ActivityItem>,
    last_update: Instant,
    bytes_since_update: u64,
}

#[derive(Clone, PartialEq)]
enum ChunkState {
    Pending,
    Downloading,
    Completed,
    Failed,
}

impl DownloadStats {
    fn new(total: usize) -> Self {
        let chunk_count = total.min(100);
        Self {
            total_segments: total,
            downloaded_segments: 0,
            failed_segments: 0,
            downloaded_bytes: 0,
            start_time: Instant::now(),
            current_speed: 0.0,
            speed_history: VecDeque::with_capacity(50),
            chunk_states: vec![ChunkState::Pending; chunk_count],
            activity_log: VecDeque::with_capacity(6),
            last_update: Instant::now(),
            bytes_since_update: 0,
        }
    }

    fn update(&mut self, segment_id: usize, bytes: u64, segment_name: String) {
        self.downloaded_segments += 1;
        self.downloaded_bytes += bytes;
        self.bytes_since_update += bytes;

        // 添加活动日志
        self.activity_log.push_back(ActivityItem {
            name: segment_name,
            status: ActivityStatus::Success,
        });
        if self.activity_log.len() > 6 {
            self.activity_log.pop_front();
        }

        // 更新速度
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        if elapsed >= 0.25 {
            self.current_speed = (self.bytes_since_update as f64) / elapsed / (1024.0 * 1024.0);
            self.speed_history.push_back(self.current_speed);
            if self.speed_history.len() > 50 {
                self.speed_history.pop_front();
            }
            self.last_update = now;
            self.bytes_since_update = 0;
        }

        // 更新分块状态
        let chunk_id = (segment_id * self.chunk_states.len()) / self.total_segments;
        if chunk_id < self.chunk_states.len() {
            self.chunk_states[chunk_id] = ChunkState::Completed;
        }
    }

    fn fail(&mut self, segment_id: usize, segment_name: String) {
        self.failed_segments += 1;

        self.activity_log.push_back(ActivityItem {
            name: segment_name,
            status: ActivityStatus::Failed,
        });
        if self.activity_log.len() > 6 {
            self.activity_log.pop_front();
        }

        let chunk_id = (segment_id * self.chunk_states.len()) / self.total_segments;
        if chunk_id < self.chunk_states.len() {
            self.chunk_states[chunk_id] = ChunkState::Failed;
        }
    }

    fn progress_percent(&self) -> f64 {
        if self.total_segments > 0 {
            (self.downloaded_segments as f64 / self.total_segments as f64) * 100.0
        } else {
            0.0
        }
    }

    fn average_speed(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            (self.downloaded_bytes as f64) / elapsed / (1024.0 * 1024.0)
        } else {
            0.0
        }
    }

    fn elapsed_time(&self) -> Duration {
        self.start_time.elapsed()
    }

    fn eta(&self) -> Option<Duration> {
        if self.average_speed() > 0.0 && self.downloaded_segments > 0 {
            let remaining = self.total_segments - self.downloaded_segments;
            let avg_size = self.downloaded_bytes as f64 / self.downloaded_segments as f64;
            let eta_seconds = (remaining as f64 * avg_size) / (self.average_speed() * 1024.0 * 1024.0);
            Some(Duration::from_secs_f64(eta_seconds))
        } else {
            None
        }
    }
}

fn draw_ui(f: &mut Frame, stats: &DownloadStats, url: &str, output: &str) {
    let size = f.size();

    // 主布局：顶部Logo + 主体
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Logo
            Constraint::Min(0),     // Main
        ])
        .split(size);

    // Logo
    let logo = Paragraph::new(Line::from(vec![
        Span::styled("S", Style::default().fg(COLOR_NEON_PURPLE).add_modifier(Modifier::BOLD)),
        Span::styled("U", Style::default().fg(COLOR_NEON_PINK).add_modifier(Modifier::BOLD)),
        Span::styled("R", Style::default().fg(COLOR_NEON_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("G", Style::default().fg(COLOR_NEON_PURPLE).add_modifier(Modifier::BOLD)),
        Span::styled("E", Style::default().fg(COLOR_NEON_PINK).add_modifier(Modifier::BOLD)),
        Span::styled(" M3U8 ", Style::default().fg(COLOR_NEON_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("Quad", Style::default().fg(COLOR_GRAY).add_modifier(Modifier::ITALIC)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(COLOR_NEON_CYAN)))
    .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(logo, chunks[0]);

    // 主体：上下分
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),  // Top
            Constraint::Percentage(50),  // Bottom
        ])
        .split(chunks[1]);

    // 上排：Info(30%) + Graph(70%)
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(main_chunks[0]);

    // 下排：Activity(30%) + Stats(20%) + ChunkMap(50%)
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(50),
        ])
        .split(main_chunks[1]);

    // Info Panel
    draw_info_panel(f, top_chunks[0], stats, url, output);

    // Speed Graph Panel
    draw_graph_panel(f, top_chunks[1], stats);

    // Activity Panel
    draw_activity_panel(f, bottom_chunks[0], stats);

    // Stats Panel
    draw_stats_panel(f, bottom_chunks[1], stats);

    // Chunk Map Panel
    draw_chunkmap_panel(f, bottom_chunks[2], stats);
}

fn draw_info_panel(f: &mut Frame, area: Rect, stats: &DownloadStats, url: &str, output: &str) {
    let url_display = if url.len() > 25 {
        format!("{}...", &url[..22])
    } else {
        url.to_string()
    };

    let progress_bar_width = 20;
    let filled = (stats.progress_percent() / 5.0) as usize;
    let progress_bar = format!("{}{}",
        "█".repeat(filled.min(progress_bar_width)),
        "░".repeat(progress_bar_width.saturating_sub(filled))
    );

    let text = vec![
        Line::from(vec![
            Span::styled("URL: ", Style::default().fg(COLOR_NEON_CYAN)),
            Span::raw(url_display),
        ]),
        Line::from(vec![
            Span::styled("Output: ", Style::default().fg(COLOR_NEON_CYAN)),
            Span::raw(format!("{}.mp4", output)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Progress: ", Style::default().fg(COLOR_NEON_CYAN)),
            Span::styled(progress_bar, Style::default().fg(COLOR_NEON_PINK)),
            Span::raw(format!(" {:.1}%", stats.progress_percent())),
        ]),
        Line::from(vec![
            Span::styled("Segments: ", Style::default().fg(COLOR_NEON_CYAN)),
            Span::styled(
                format!("{}", stats.downloaded_segments),
                Style::default().fg(COLOR_COMPLETED)
            ),
            Span::raw("/"),
            Span::raw(format!("{}", stats.total_segments)),
            if stats.failed_segments > 0 {
                Span::styled(
                    format!(" ({}✗)", stats.failed_segments),
                    Style::default().fg(COLOR_FAILED)
                )
            } else {
                Span::raw("")
            },
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_NEON_PINK))
                .title(Span::styled("Info", Style::default().fg(COLOR_NEON_CYAN).add_modifier(Modifier::BOLD)))
        );
    f.render_widget(paragraph, area);
}

fn draw_graph_panel(f: &mut Frame, area: Rect, stats: &DownloadStats) {
    let max_speed = stats.speed_history.iter().cloned().fold(0.0f64, f64::max).max(1.0);
    let avg_speed = stats.average_speed();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("▼ Speed  ", Style::default().fg(COLOR_NEON_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("Peak: {:.2} MB/s  ", max_speed), Style::default().fg(COLOR_NEON_PINK)),
            Span::styled(format!("Avg: {:.2} MB/s", avg_speed), Style::default().fg(COLOR_NEON_PURPLE)),
        ]),
    ];

    // 绘制速度图表
    let graph_height = (area.height as usize).saturating_sub(4).max(6);
    let graph_width = (area.width as usize).saturating_sub(4).max(20);

    let block_chars = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    for row in (0..graph_height).rev() {
        let threshold = ((row + 1) as f64 / graph_height as f64) * max_speed;
        let mut line_spans = Vec::new();

        let points: Vec<f64> = stats.speed_history.iter().cloned().collect();
        let display_points = if points.len() > graph_width {
            &points[points.len() - graph_width..]
        } else {
            &points[..]
        };

        for &speed in display_points {
            if speed >= threshold {
                let ratio = (speed - (threshold - max_speed / graph_height as f64)) / (max_speed / graph_height as f64);
                let block_idx = (ratio * 8.0) as usize;
                let block_idx = block_idx.min(8);
                let ch = block_chars[block_idx];

                let color = if speed > max_speed * 0.7 {
                    COLOR_NEON_PINK
                } else if speed > max_speed * 0.4 {
                    COLOR_NEON_PURPLE
                } else {
                    COLOR_NEON_CYAN
                };

                line_spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
            } else {
                line_spans.push(Span::raw(" "));
            }
        }

        lines.push(Line::from(line_spans));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_NEON_CYAN))
        );
    f.render_widget(paragraph, area);
}

fn draw_activity_panel(f: &mut Frame, area: Rect, stats: &DownloadStats) {
    let lines: Vec<Line> = if stats.activity_log.is_empty() {
        vec![Line::from(Span::styled("Waiting...", Style::default().fg(COLOR_GRAY)))]
    } else {
        stats.activity_log.iter().map(|item| {
            let (icon, color) = match item.status {
                ActivityStatus::Success => ("✓ ", COLOR_COMPLETED),
                ActivityStatus::Failed => ("✗ ", COLOR_FAILED),
                ActivityStatus::Downloading => ("⟳ ", COLOR_NEON_CYAN),
            };

            let name = if item.name.len() > 20 {
                format!("{}...", &item.name[..17])
            } else {
                item.name.clone()
            };

            Line::from(vec![
                Span::styled(icon, Style::default().fg(color)),
                Span::raw(name),
            ])
        }).collect()
    };

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_NEON_PURPLE))
                .title(Span::styled("Activity", Style::default().fg(COLOR_NEON_CYAN).add_modifier(Modifier::BOLD)))
        );
    f.render_widget(paragraph, area);
}

fn draw_stats_panel(f: &mut Frame, area: Rect, stats: &DownloadStats) {
    let elapsed = stats.elapsed_time();
    let eta = stats.eta();

    let lines = vec![
        Line::from(vec![
            Span::styled("Speed: ", Style::default().fg(COLOR_NEON_CYAN)),
            Span::styled(format!("{:.1}", stats.current_speed), Style::default().fg(COLOR_NEON_PINK).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Down: ", Style::default().fg(COLOR_NEON_CYAN)),
            Span::styled(
                format!("{} MB", stats.downloaded_bytes / (1024 * 1024)),
                Style::default().fg(COLOR_NEON_PINK).add_modifier(Modifier::BOLD)
            ),
        ]),
        Line::from(vec![
            Span::styled("Time: ", Style::default().fg(COLOR_NEON_CYAN)),
            Span::styled(
                format!("{}m{}s", elapsed.as_secs() / 60, elapsed.as_secs() % 60),
                Style::default().fg(COLOR_NEON_PINK).add_modifier(Modifier::BOLD)
            ),
        ]),
        if let Some(eta_duration) = eta {
            Line::from(vec![
                Span::styled("ETA: ", Style::default().fg(COLOR_NEON_CYAN)),
                Span::styled(
                    format!("{}m{}s", eta_duration.as_secs() / 60, eta_duration.as_secs() % 60),
                    Style::default().fg(COLOR_NEON_PINK).add_modifier(Modifier::BOLD)
                ),
            ])
        } else {
            Line::from("")
        },
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_NEON_PURPLE))
                .title(Span::styled("Stats", Style::default().fg(COLOR_NEON_CYAN).add_modifier(Modifier::BOLD)))
        );
    f.render_widget(paragraph, area);
}

fn draw_chunkmap_panel(f: &mut Frame, area: Rect, stats: &DownloadStats) {
    let chunks_per_row = ((area.width as usize).saturating_sub(2)) / 2;
    let mut lines = Vec::new();
    let mut current_line = Vec::new();

    for (i, state) in stats.chunk_states.iter().enumerate() {
        if i > 0 && i % chunks_per_row == 0 {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
        }

        let (color, _) = match state {
            ChunkState::Completed => (COLOR_COMPLETED, "■ "),
            ChunkState::Downloading => (COLOR_NEON_PINK, "■ "),
            ChunkState::Failed => (COLOR_FAILED, "■ "),
            ChunkState::Pending => (COLOR_GRAY, "■ "),
        };

        current_line.push(Span::styled("■ ", Style::default().fg(color)));
    }

    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(COLOR_NEON_PURPLE))
                .title(Span::styled("Chunks", Style::default().fg(COLOR_NEON_CYAN).add_modifier(Modifier::BOLD)))
        );
    f.render_widget(paragraph, area);
}

struct M3U8Downloader {
    url: String,
    output_dir: PathBuf,
    temp_dir: PathBuf,
    client: reqwest::Client,
    concurrent_limit: usize,
}

impl M3U8Downloader {
    fn new(url: String, output_dir: PathBuf, concurrent_limit: usize) -> Self {
        let temp_dir = output_dir.join("temp");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            url,
            output_dir,
            temp_dir,
            client,
            concurrent_limit,
        }
    }

    async fn fetch_m3u8(&self) -> Result<Vec<String>> {
        println!("📡 正在解析M3U8文件...");

        let response = self.client.get(&self.url).send().await?;
        let content = response.text().await?;
        let parsed = m3u8_rs::parse_playlist_res(content.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to parse M3U8: {:?}", e))?;

        let segments = match parsed {
            Playlist::MasterPlaylist(pl) => {
                let best_variant = pl.variants.iter().max_by_key(|v| v.bandwidth)
                    .context("No variants found")?;

                let variant_url = self.resolve_url(&best_variant.uri)?;
                println!("  ✓ 选择最高质量流");

                let response = self.client.get(&variant_url).send().await?;
                let content = response.text().await?;
                let parsed = m3u8_rs::parse_playlist_res(content.as_bytes())
                    .map_err(|e| anyhow::anyhow!("Failed to parse: {:?}", e))?;

                match parsed {
                    Playlist::MediaPlaylist(media_pl) => media_pl.segments.iter()
                        .map(|seg| self.resolve_url(&seg.uri))
                        .collect::<Result<Vec<_>>>()?,
                    _ => anyhow::bail!("Invalid media playlist"),
                }
            }
            Playlist::MediaPlaylist(pl) => {
                pl.segments.iter()
                    .map(|seg| self.resolve_url(&seg.uri))
                    .collect::<Result<Vec<_>>>()?
            }
        };

        println!("  ✓ 找到 {} 个视频片段\n", segments.len());
        Ok(segments)
    }

    fn resolve_url(&self, uri: &str) -> Result<String> {
        let base_url = Url::parse(&self.url)?;
        let resolved = base_url.join(uri)?;
        Ok(resolved.to_string())
    }

    async fn download_segments(
        &self,
        segments: Vec<String>,
        stats: Arc<Mutex<DownloadStats>>,
    ) -> Result<()> {
        fs::create_dir_all(&self.temp_dir).await?;

        let downloader = Arc::new(self);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.concurrent_limit));

        stream::iter(segments.into_iter().enumerate())
            .for_each_concurrent(None, |(i, url)| {
                let downloader = Arc::clone(&downloader);
                let stats = Arc::clone(&stats);
                let semaphore = Arc::clone(&semaphore);

                async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    let output_path = downloader.temp_dir.join(format!("segment_{:05}.ts", i));
                    let segment_name = format!("segment_{:05}.ts", i);

                    match downloader.download_segment(&url, &output_path).await {
                        Ok(bytes) => {
                            let mut stats = stats.lock().await;
                            stats.update(i, bytes, segment_name);
                        }
                        Err(_) => {
                            let mut stats = stats.lock().await;
                            stats.fail(i, segment_name);
                        }
                    }
                }
            })
            .await;

        Ok(())
    }

    async fn download_segment(&self, url: &str, output_path: &PathBuf) -> Result<u64> {
        let response = self.client.get(url).send().await?;
        let bytes = response.bytes().await?;
        let len = bytes.len() as u64;

        let mut file = File::create(output_path).await?;
        file.write_all(&bytes).await?;

        Ok(len)
    }

    async fn merge_to_mp4(&self, output_name: &str) -> Result<PathBuf> {
        let filelist_path = self.temp_dir.join("filelist.txt");

        let mut ts_files = Vec::new();
        let mut read_dir = fs::read_dir(&self.temp_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            ts_files.push(entry);
        }

        ts_files.sort_by_key(|e| e.file_name());

        let mut filelist_content = String::new();
        for entry in ts_files {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("ts") {
                let abs_path = entry.path().canonicalize()?;
                filelist_content.push_str(&format!("file '{}'\n", abs_path.display()));
            }
        }

        tokio::fs::write(&filelist_path, filelist_content).await?;

        let output_path = self.output_dir.join(format!("{}.mp4", output_name));

        println!("\n🎬 正在合并视频片段...");

        let status = Command::new("ffmpeg")
            .args(&[
                "-f", "concat",
                "-safe", "0",
                "-i", &filelist_path.to_string_lossy(),
                "-c", "copy",
                "-y",
                &output_path.to_string_lossy(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;

        if !status.success() {
            anyhow::bail!("FFmpeg failed");
        }

        println!("✓ 成功: {}\n", output_path.display());

        Ok(output_path)
    }

    async fn cleanup(&self) -> Result<()> {
        if self.temp_dir.exists() {
            tokio::fs::remove_dir_all(&self.temp_dir).await?;
        }
        Ok(())
    }
}

async fn run_tui(
    stats: Arc<Mutex<DownloadStats>>,
    url: String,
    output: String,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    loop {
        {
            let stats_guard = stats.lock().await;
            terminal.draw(|f| draw_ui(f, &stats_guard, &url, &output))?;

            // 检查是否完成
            if stats_guard.downloaded_segments + stats_guard.failed_segments >= stats_guard.total_segments {
                break;
            }
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // 恢复终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let output_dir = expand_path(&args.dir);
    fs::create_dir_all(&output_dir).await?;

    let downloader = M3U8Downloader::new(
        args.url.clone(),
        output_dir,
        args.concurrent,
    );

    let segments = downloader.fetch_m3u8().await?;
    let stats = Arc::new(Mutex::new(DownloadStats::new(segments.len())));

    // 启动 TUI
    let tui_stats = Arc::clone(&stats);
    let tui_url = args.url.clone();
    let tui_output = args.output.clone();
    let tui_handle = tokio::spawn(async move {
        run_tui(tui_stats, tui_url, tui_output).await
    });

    // 下载
    downloader.download_segments(segments, Arc::clone(&stats)).await?;

    // 等待 TUI 完成
    tokio::time::sleep(Duration::from_secs(1)).await;
    tui_handle.abort();

    let final_stats = stats.lock().await;
    if final_stats.failed_segments > 0 {
        println!("⚠ 警告: {} 个片段下载失败", final_stats.failed_segments);
    }

    drop(final_stats);

    let output_file = downloader.merge_to_mp4(&args.output).await?;
    downloader.cleanup().await?;

    let size_mb = output_file.metadata()?.len() as f64 / (1024.0 * 1024.0);
    println!("✓ 文件: {}", output_file.display());
    println!("✓ 大小: {:.2} MB", size_mb);

    Ok(())
}
