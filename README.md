> **This is a fork.** It adds [`qupath-rs/`](qupath-rs/), an experimental Rust/[egui](https://www.egui.rs)/[fastframe](https://fastframe.dev)
> reimplementation of QuPath's interface that reads and writes QuPath projects and GeoJSON.
> In benchmarks it detects cells about 12× faster than QuPath 0.7.0, with the same results.
> AI coding agents (Claude Code, Codex, Cursor, Gemini CLI) can drive it through its MCP plugin; see [AI agents](#ai-agents-qupath-rs-for-claude-code-codex--co).
> It is written with generative AI, is not affiliated with the QuPath team, and is not
> intended for upstream (see [AI_POLICY.md](AI_POLICY.md)). Everything else is upstream QuPath, unchanged.

[![Image.sc forum](https://img.shields.io/badge/dynamic/json.svg?label=forum&url=https%3A%2F%2Fforum.image.sc%2Ftags%2Fqupath.json&query=%24.topic_list.tags.0.topic_count&colorB=brightgreen&suffix=%20topics&logo=data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAA4AAAAOCAYAAAAfSC3RAAABPklEQVR42m3SyyqFURTA8Y2BER0TDyExZ+aSPIKUlPIITFzKeQWXwhBlQrmFgUzMMFLKZeguBu5y+//17dP3nc5vuPdee6299gohUYYaDGOyyACq4JmQVoFujOMR77hNfOAGM+hBOQqB9TjHD36xhAa04RCuuXeKOvwHVWIKL9jCK2bRiV284QgL8MwEjAneeo9VNOEaBhzALGtoRy02cIcWhE34jj5YxgW+E5Z4iTPkMYpPLCNY3hdOYEfNbKYdmNngZ1jyEzw7h7AIb3fRTQ95OAZ6yQpGYHMMtOTgouktYwxuXsHgWLLl+4x++Kx1FJrjLTagA77bTPvYgw1rRqY56e+w7GNYsqX6JfPwi7aR+Y5SA+BXtKIRfkfJAYgj14tpOF6+I46c4/cAM3UhM3JxyKsxiOIhH0IO6SH/A1Kb1WBeUjbkAAAAAElFTkSuQmCC)](https://forum.image.sc/tag/qupath)
[![Total downloads](https://img.shields.io/github/downloads/qupath/qupath/total?style=flat)](https://github.com/qupath/qupath/releases)
[![Latest release downloads](https://img.shields.io/github/downloads/qupath/qupath/latest/total?style=flat)](https://github.com/qupath/qupath/releases/latest)
[![Paper](https://zenodo.org/badge/DOI/10.1038/s41598-017-17204-5.svg)](https://doi.org/10.1038/s41598-017-17204-5)
[![Twitter](https://img.shields.io/twitter/follow/qupath?style=flat)](http://twitter.com/qupath)

# QuPath

**QuPath is open source software for bioimage analysis**.

Features include:

* Lots of tools to annotate and view images, including whole slide & microscopy images
* Workflows for brightfield & fluorescence image analysis
* New algorithms for common tasks, including cell segmentation, tissue microarray dearraying
* Interactive machine learning for object & pixel classification
* Customization, batch-processing & data interrogation by scripting
* Easy integration with other tools, including ImageJ

To **download QuPath**, go to the [Latest Releases](https://github.com/qupath/qupath/releases/latest) page.

For **documentation**, see [https://qupath.readthedocs.io](https://qupath.readthedocs.io)

For **help & support**, try [image.sc](https://forum.image.sc/tag/qupath) or the [links here](https://qupath.readthedocs.io/en/stable/docs/starting/help.html)

To **build QuPath from source** see [here](https://qupath.readthedocs.io/en/stable/docs/reference/building.html).

**If you find QuPath useful in work that you publish, please [_cite the publication_](https://qupath.readthedocs.io/en/stable/docs/intro/citing.html)!**

*QuPath is an academic project intended for research use only.*
*The software has been made freely available under the terms of the [GPLv3](https://github.com/qupath/qupath/blob/main/LICENSE) in the hope it is useful for this purpose, and to make analysis methods open and transparent.*


## Installation

### Install a release

Download the installer for your platform from the [Latest Releases](https://github.com/qupath/qupath/releases/latest) page and follow the [installation guide](https://qupath.readthedocs.io/en/stable/docs/intro/installation.html).

### Build QuPath from source

You need [Git](https://git-scm.com) and a Java Development Kit (JDK 17 or later) to run Gradle.
QuPath itself is built with JDK 25 (see [`gradle/libs.versions.toml`](gradle/libs.versions.toml)); if it isn't installed, Gradle downloads it automatically.

On macOS, you can install everything you need for both apps with [Homebrew](https://brew.sh) (QuPath itself isn't available as a Homebrew package):

```bash
brew install git rust openslide
brew install --cask temurin@25
```

```bash
git clone https://github.com/TobiasPol/qupath.git
cd qupath
./gradlew clean build      # compile and run tests
./gradlew run              # start QuPath
```

On Windows, use `gradlew.bat` instead of `./gradlew`.

To create a standalone app in `build/dist/`:

```bash
./gradlew clean jpackage                       # app image (default)
./gradlew clean jpackage -Ppackage=installer   # native installer (.pkg / .msi / .deb)
```

See [TECHNICAL_NOTES.md](TECHNICAL_NOTES.md) and the [building docs](https://qupath.readthedocs.io/en/stable/docs/reference/building.html) for more options, including running from an IDE.

### Download qupath-rs (experimental)

Download the build for your platform from this fork's [Releases](https://github.com/TobiasPol/qupath/releases) page (releases tagged `qupath-rs-v…`):

| Platform | File | How to run |
| --- | --- | --- |
| macOS (Apple Silicon & Intel) | `QuPath-rs-<version>-macOS-universal.zip` | Unzip and move `QuPath-rs.app` to Applications |
| Linux (x86_64) | `QuPath-rs-<version>-linux-x86_64.tar.gz` | Extract and run `./qupath-rs` |
| Windows (x86_64) | `QuPath-rs-<version>-windows-x86_64.zip` | Extract and run `qupath-rs.exe` |

The builds are not notarised or code-signed with a developer certificate, so your OS will warn you the first time:

* **macOS**: open the app once, then choose *Open Anyway* in System Settings › Privacy & Security, or run `xattr -dr com.apple.quarantine /Applications/QuPath-rs.app`
* **Windows**: in the SmartScreen prompt, choose *More info* › *Run anyway*

Only the macOS build is tested regularly. Vendor formats (NDPI, MRXS, …) need OpenSlide installed separately (`brew install openslide` on macOS, `apt install libopenslide1` on Debian/Ubuntu).

To publish a new release, push a tag such as `qupath-rs-v0.1.0`. The [release workflow](.github/workflows/qupath-rs-release.yml) builds all three platforms and drafts a GitHub release for you to review and publish.

### Build qupath-rs from source

The Rust app in [`qupath-rs/`](qupath-rs/) needs [Rust](https://rustup.rs) 1.98 or later.

```bash
cd qupath-rs
cargo run --release -- path/to/slide.svs   # the argument is optional
```

On macOS, `scripts/bundle-macos.sh` builds `dist/QuPath-rs.app`.
Vendor formats (NDPI, MRXS, …) need OpenSlide (`brew install openslide`).
See [qupath-rs/README.md](qupath-rs/README.md) for details.


## AI agents: qupath-rs for Claude Code, Codex & co.

qupath-rs has a built-in [Model Context Protocol](https://modelcontextprotocol.io) server, `qupath-rs mcp`, and a plugin for Claude Code, Codex, Cursor and Gemini CLI. You describe an analysis in plain words, and the agent can:

* open whole-slide images and QuPath projects, and look at regions as images with the objects drawn on top
* detect tissue, draw annotations, and run QuPath's cell and positive cell detection
* read measurements with QuPath's column names, and export GeoJSON, CSV, or a Groovy script that reruns the analysis in QuPath

The agent saves its work as an ordinary QuPath project, so you can review every annotation and cell in QuPath 0.7 or the qupath-rs GUI.

**Setup**

1. Install the qupath-rs binary: [download a release](#download-qupath-rs-experimental) or [build it from source](#build-qupath-rs-from-source).
2. Add the plugin to your agent.

   Claude Code:

   ```bash
   /plugin install qupath-rs --marketplace TobiasPol/qupath
   ```

   Codex:

   ```bash
   codex plugin marketplace add TobiasPol/qupath
   ```

   ```bash
   codex plugin add qupath-rs@qupath-rs
   ```

   For Cursor, Gemini CLI and other MCP clients (VS Code, Windsurf, Claude Desktop…), see the [plugin README](qupath-rs/plugin/README.md).

**Example request**

> Create a project in ~/analysis/ki67, add the slides in ~/slides, detect the tissue on each one, quantify Ki-67 positive cells and give me a table of positive % per slide.

The plugin's [`qupath-analysis` skill](qupath-rs/plugin/skills/qupath-analysis/SKILL.md) teaches the agent the workflow and how to tune the parameters. It also has the agent check its detections visually before reporting.

Notes:

* Results come from qupath-rs's port of QuPath's algorithms. They are for research use only, not for diagnosis.
* Exports never overwrite existing files unless the agent is explicitly asked to.


## Development & support

QuPath is being actively developed at the University of Edinburgh by:

* [Pete Bankhead](https://github.com/petebankhead) (creator)
* [Alan O'Callaghan](https://github.com/alanocallaghan)
* [Laura Nicolás-Sáenz](https://github.com/lauranicolass)

with thanks to past QuPath dev team members:

* [Léo Leplat](https://github.com/Rylern)
* [Fiona Inglis](https://github.com/finglis)
* Melvin Gelbard
* Mahdi Lamb

For all contributors, see [here](https://github.com/qupath/qupath/graphs/contributors).

This work is made possible in part thanks to funding from:

* [Wellcome Trust Technology Development Grant](https://wellcome.org/grant-funding/people-and-projects/grants-awarded/qupath-advanced-open-platform-next-generation) (2022-Present)
* [CZI Essential Open Source Software for Science, Cycle 4](https://chanzuckerberg.com/eoss/proposals/qupath-boosting-bioimage-analysis-for-users-developers/) (2022-2025)
* [CZI Essential Open Source Software for Science, Cycle 1](https://chanzuckerberg.com/eoss/proposals/qupath-open-source-bioimage-analysis-and-quantitative-pathology/) (2020-2022)
* Wellcome Trust / University of Edinburgh Institutional Strategic Support Fund (ISSF3) (2019-2020)


----

## Background

QuPath was first designed, implemented and documented by Pete Bankhead while at Queen's University Belfast, with additional code and testing by Jose Fernandez.

Versions up to v0.1.2 are copyright 2014-2016 The Queen's University of Belfast, Northern Ireland.
These were written as part of projects that received funding from:

* Invest Northern Ireland (RDO0712612)
* Cancer Research UK Accelerator (C11512/A20256)


![Image](https://raw.githubusercontent.com/wiki/qupath/qupath/images/qupath_demo.jpg)
