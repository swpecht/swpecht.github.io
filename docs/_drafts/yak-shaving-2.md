---
layout: post
title:  "Yak shaving I did to get set up, part 2"
date:   2021-02-09 02:00:43 +0000
categories: project-log
---

## Summary
Switched to VS Code and code-server.


* Having an issue with the rust extension and VS code. Trying troubleshooting outlined here: https://github.com/rust-lang/vscode-rust/issues/237#issuecomment-478299249
  *  Issue solved by adding VS code setting: `"rust-client.rustupPath": "/home/username/.cargo/bin/rustup"`