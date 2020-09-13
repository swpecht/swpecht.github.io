---
layout: post
title:  "Voice chat exploration"
date:   2020-05-20 02:00:43 +0000
categories: project-log
---
Things I did in. Roughly the order I did them. And why.

### Get some webRTC PoC up and running (May 18, 2020)
Why: See if this is even remotely possible.

Send stream of microphone data to server, and then back to browser. [ref](https://github.com/pion/webrtc/tree/master/examples/reflect)
* Needed to manually set `sessionID` before compilation rather than using stdin
* Need to have ssl or [treat ip as secure](chrome://flags/#unsafely-treat-insecure-origin-as-secure), otherwise can't get mic access.

Other useful links
* [Example video call site using WebRTC](https://webrtc.github.io/samples/src/content/peerconnection/pc1/)
* [Go library for WebRTC and streaming](https://github.com/pion/webrtc/tree/master/examples/save-to-disk)

### Get static file server up and running (May 18, 2020)
Why: Need some way to serve content I'm developing
How: Go can do this pretty easily out of the box [ref](https://www.alexedwards.net/blog/serving-static-sites-with-go)
Serving the reflect example page [ref](https://github.com/pion/webrtc/tree/master/examples/reflect)

### Get the page to connect to server with one-click (May 22, 2020)
*  Post something to the server and print to console
*  Post the connection JSON to the server
*  Receive back the server connection JSON
*  open the connection

### Enable reflect with separate tracks for audio in and out (path to multiple clients) (May 23, 2020)
*  Find an example of this [possible](https://github.com/webrtc/samples/blob/gh-pages/src/content/peerconnection/multiple/js/main.js), [pion broadcast, this seems to have what's needed](https://github.com/pion/webrtc/blob/master/examples/broadcast/main.go)
*  eventually want to explore how to mix the tracks? Or send all the audio tracks across? Is there a way to adjust volume? [mixing example](https://stackoverflow.com/questions/42138545/webrtc-mix-local-and-remote-audio-steams-and-record) -- [can't do multiple tracks per connection](https://github.com/microsoft/MixedReality-WebRTC/issues/144). instead will do multiple connections for each person
*  [multi connection example](https://github.com/webrtc/samples/blob/gh-pages/src/content/peerconnection/audio/js/main.js)
*  TODO: add param for new connection on type of connection
*  Stuck on getting javascript to do multiple connections -- may need to make them synchronous
*  trying to do multiple connections seems to create problems. Got it working on the client side, but w/o a 2 way track have issues triggering all needed events. Going to try to use a single connection with late added tracks
* Hitting an issue trying to get tracks to register, seems to be a problem with not properly handlign the re-negotiation.
* Good lead on how to do it: https://github.com/pion/webrtc/tree/master/examples/play-from-disk-renegotation
* this actually has everything, including no-op data channel to force a connection even if don't yet have tracks
* got the noop connection up, now need trying to make sure the audio track is included. Then can make sure the client knows when output audio available.
* got re-negotioation working, now need to figure out how to properly get the output track and the input track, I see that I have 2 tracks, but the OnTrack isn't called for the initial audio input track for some reason.
Had to use a number of different pion axample

### Refactor copy loop into a separate go func that waits to activate (May 28, 2020)
*  Goal is to make it easier to manage threads later.

### Move audio stream handling to constantly running go loop (June 7, 2020)
Goal: make it clear based on id and explicit track storing rather than a callback as to how things are copied over. Unclear how the callbacks are currently working.

Implemented the separate tracks w/ ids. but still can't separate the input and output stream. Going to move to a single track per connection.

### Use separate peer connection for input and output audio (June 7, 2020)



### Enable audio chat with multiple clients (TBD)
[longer term goal, see steps above for smaller chunk of work]

Cur: trying to refactor out the track logic in go. Seems like the 'OnTrack' call isn't quite working as intended. Pausing the input track in the browser doesn't seem to pause the stream of audio. Perhaps it's not part of the second track?

May be seeing an issue with the id -- everything is pion. Updating to different track id's doesn't help

*  signal some sort of ID (input or randomly generated) -- done
*  set up golang to handle instance to id, then can plumb up the proper sending of audio in between clients
*  start with reflecting everything, but eventually more
*  Can create golang map<id>{audioIn, audioOut}
