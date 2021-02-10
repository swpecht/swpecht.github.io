---
layout: post
title:  "Rendered MVP"
date:   2021-09-10 02:00:43 +0000
categories: project-log
---

### Summary
Create a rust-wasm renderer and do some bechmarks on a custom encoding scheme.

### Milestone 1: Get some pixels on the screen
Goal: Paint pixels to a canvas using rust-wasm.

*  Get WASM toolchain set up using the [Mozilla docs](https://developer.mozilla.org/en-US/docs/WebAssembly/Rust_to_wasm)
  *  Needed to first do: `sudo apt-get install libssl-dev`
* Got the 'hello world' example up and running
