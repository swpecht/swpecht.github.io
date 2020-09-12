---
layout: post
title:  "Video encoding and streaming proof of concept"
date:   2020-09-10 02:00:43 +0000
categories: project-log
---

## Context
 Want do build a simulation of a robot slowly exploring a room (like a roomba). This will run in some sort of physics engine -- TBD if game engine or custom built. Need a way to view the state of the physics simulation. This post covers a possible solution where we stream the simulation as a video stream to a browser.
  
## Goals
Create a simplified proof of concept for video encoding and streaming using Go and WASM.
*  Comparison of bandwidth between raw pixel stream and diff-based stream
  
# Requirements
* *Raw pixel streaming:* Server that can stream raw pixel data and a browser client to receive the data
* *Video presentation:* Client capable of taking raw pixel data and painting it to a browser windows at 30fps
* *Encoded video streaming:* Server that can do a diff based encode of a frame based on previous frame. And a client that can decode the diff-based encoding.                    

# Non-goals
*  adaptive streaming 
*  worrying about re-starting streams or re-negotiating i-frames 
*  complicated encoder logic 

## How
*  Use WASM: can write all code in Go [details](https://github.com/golang/go/wiki/WebAssembly)
*  Paint directly into an HTML canvas, don't need to worry about javascript libraries for playing video. This is likely fine since most frame will be similar to the last frames [example](https://www.hellorust.  com/demos/canvas/index.html)

## Results
TBD
