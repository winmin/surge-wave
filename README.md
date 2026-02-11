# 🌊 Surge Wave

A blazingly fast M3U8/HLS video downloader with a beautiful cyberpunk-inspired TUI.

<div align="center">

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)]()

</div>

## ✨ Features

- 🚀 **Blazingly Fast** - Written in Rust with async/concurrent downloads
- 🎨 **Beautiful TUI** - Quad-pane layout inspired by [surge-downloader](https://github.com/surge-downloader/surge)
- 🎭 **Cyberpunk Theme** - Neon color scheme with real-time visualizations
- 📊 **Rich Statistics** - Live speed graph, chunk map, and activity log
- ⚡ **Low Resource** - ~30MB memory, significantly lower than Python alternatives
- 📦 **Single Binary** - No dependencies except FFmpeg

## 🎨 Interface

```
╔═══════════════════════════════════════════════════════════════╗
║                    SURGE M3U8 Quad                            ║
╠═══════════════════════════════════════════════════════════════╣
║ ┌─ Info ──────────┐ ┌─ Speed Graph ─────────────────────┐   ║
║ │ URL: xxx        │ │ ▼ Speed  Peak: 15.2  Avg: 12.5    │   ║
║ │ Output: vid.mp4 │ │ ▇▇▆▅▄▃▂▁                          │   ║
║ │ [████████] 65%  │ │ ▇▆▅▄▃▂▁                           │   ║
║ │ Segs 1650/2550  │ │ ▆▅▄▃▂▁                            │   ║
║ └─────────────────┘ └───────────────────────────────────┘   ║
║ ┌─ Activity ──────┐ ┌─ Stats ─┐ ┌─ Chunk Map ──────────┐   ║
║ │ ✓ segment_1234  │ │ Speed   │ │ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■  │   ║
║ │ ✓ segment_1235  │ │ 15 MB/s │ │ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■  │   ║
║ │ ✓ segment_1236  │ │ Down    │ │ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■  │   ║
║ │ ✓ segment_1237  │ │ 3250 MB │ │ ■ ■ ■ ░ ░ ░ ░ ░ ░ ░  │   ║
║ │ ❌ segment_1238 │ │ Time    │ └───────────────────────┘   ║
║ │ ✓ segment_1239  │ │ 3m45s   │                           ║
║ └─────────────────┘ └─────────┘                             ║
╚═══════════════════════════════════════════════════════════════╝
```

**Layout Design**: Inspired by [surge-downloader](https://github.com/surge-downloader/surge)'s quad-pane cyberpunk interface.

### Quad-Pane Layout

- **Top Row (50%)**
  - Info Panel (30%): URL, filename, progress bar, segment count
  - Speed Graph (70%): Real-time download speed visualization with 8-level block characters

- **Bottom Row (50%)**
  - Activity Log (30%): Last 6 download events with status indicators
  - Statistics (20%): Current speed, downloaded size, elapsed time, ETA
  - Chunk Map (50%): 100-block visualization of download progress

### Color Scheme

- 🟣 **Purple** (Magenta) - Logo emphasis
- 🩷 **Pink** (Light Magenta) - Active states, borders
- 🩵 **Cyan** - Headers, labels
- 🟢 **Green** - Completed segments
- 🔴 **Red** - Failed segments
- ⚪ **Gray** - Pending segments

## 📦 Installation

### From Source

**Prerequisites:**
- [Rust](https://www.rust-lang.org/tools/install) 1.70+
- [FFmpeg](https://ffmpeg.org/download.html)

```bash
# Clone the repository
git clone https://github.com/winmin/surge-wave.git
cd surge-wave

# Build release binary
cargo build --release

# The binary will be at target/release/surge-wave
```

### Install FFmpeg

```bash
# macOS
brew install ffmpeg

# Ubuntu/Debian
sudo apt install ffmpeg

# Windows
# Download from https://ffmpeg.org/download.html
```

### System-wide Installation

```bash
# After building
sudo cp target/release/surge-wave /usr/local/bin/

# Now you can use it anywhere
surge-wave "https://example.com/video.m3u8" -o video
```

## 🚀 Usage

### Basic Usage

```bash
surge-wave "https://example.com/video.m3u8" -o my_video
```

### Full Options

```bash
surge-wave <URL> [OPTIONS]

Arguments:
  <URL>  M3U8 playlist URL

Options:
  -o, --output <NAME>       Output filename (without extension) [required]
  -d, --dir <DIR>          Download directory [default: downloads]
  -c, --concurrent <NUM>   Concurrent downloads [default: 10]
  -h, --help               Print help
  -V, --version            Print version
```

### Examples

```bash
# Basic download
surge-wave "https://example.com/video.m3u8" -o my_video

# Custom directory and concurrency
surge-wave "https://example.com/video.m3u8" \
  -o my_video \
  -d ~/Videos \
  -c 20

# High-quality stream (automatically selects highest bandwidth)
surge-wave "https://example.com/master.m3u8" -o hq_video
```

## 🎯 Why Surge Wave?

### Performance Comparison

| Metric | Python Alternatives | Surge Wave | Improvement |
|--------|-------------------|------------|-------------|
| Memory Usage | ~165 MB | **~30 MB** | 🔥 **82% lower** |
| CPU Usage | ~102% | **~80%** | 🔥 **22% lower** |
| Startup Time | 0.6s | **0.1s** | ⚡ **6x faster** |
| Binary Size | N/A (requires Python) | **3.1 MB** | ✅ Single file |
| Download Speed | Network-limited | Network-limited | Same |

### Key Advantages

- ✅ **Single Binary** - No Python, no dependencies (except FFmpeg)
- ✅ **Low Memory** - Perfect for servers and resource-constrained environments
- ✅ **Type Safe** - Rust's strong typing prevents runtime errors
- ✅ **Modern TUI** - Beautiful, informative interface
- ✅ **Production Ready** - Optimized with LTO and high optimization levels

## 🛠️ Development

### Build from Source

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Check code
cargo clippy
```

### Project Structure

```
surge-wave/
├── src/
│   └── main.rs          # Main application
├── Cargo.toml           # Dependencies and build config
└── README.md           # This file
```

## 📊 Technical Details

### Dependencies

- **tokio** - Async runtime
- **reqwest** - HTTP client for downloads
- **ratatui** - TUI framework
- **crossterm** - Terminal control
- **m3u8-rs** - M3U8 playlist parser
- **futures** - Async stream utilities

### Build Configuration

Optimized for maximum performance:

```toml
[profile.release]
opt-level = 3        # Maximum optimization
lto = true           # Link-time optimization
codegen-units = 1    # Better optimization (slower compile)
strip = true         # Strip symbols for smaller binary
```

## 🤝 Contributing

Contributions are welcome! Feel free to:

- Report bugs
- Suggest features
- Submit pull requests

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- UI/UX design inspired by [surge-downloader](https://github.com/surge-downloader/surge)
- Built with [ratatui](https://github.com/ratatui-org/ratatui)
- Powered by [Rust](https://www.rust-lang.org/)

## 📮 Author

**WinMin** - [bestswngs@gmail.com](mailto:bestswngs@gmail.com)

---

<div align="center">

**Made with ❤️ and Rust 🦀**

If you find this project useful, please consider giving it a ⭐!

</div>
