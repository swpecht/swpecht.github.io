---
layout: post
title:  "Yak shaving I did to get set up"
date:   2020-05-20 02:00:43 +0000
categories: project-log
---
This post gives a summary of what I did to get my dev environment set up. Main goals:
*  Hold myself accountable to not spend too much time tuning the colors of my terminal
*  Document, in case I need to do it again
*  Better learn Vim to write this

# Dev environment
I wanted a Linux environment for development. But I only have access to a machine running windows and ChromeOS. Dual-booting -- or something like cygwin -- is possible but it seemed like a hassle to setup. And would add another barrier to writing or working on side projects.

I ended up going with a Digital Ocean machine. It had a few added benefits for me:
*  easy backups I could turn on with a click
*  ability to access my dev environment where ever I was and a variety of different computers
*  seemed like an interesting project to try out -- I'd wanted a forcing function to get better with a terminal for a while

I'm sure any cloud provider would have worked, but I had a good experience using digital ocean in the past -- it's reasonably priced and reasonably simple.

Next, I setup tmux and putty. This ended up being a bigger waste of time than expected. I had foolishly gotten my Vim and terminal color scheme set up with out tmux. Running things through tmux immediately reverted me to the previous, ugly colors :(

I eventually realized I needed to start tmux with the `-2` argument to enable 256 colors:
```
tmux -2 a -t base || tmux -2 new -s base
```

I was able to get Putty set up with minimal configuration.

# Editor
For my editor, I went with Vim. I'd always heard great things. And the flexibility it provides seemed valuable to support both coding and blog writing in a single platform. I also appreciated the flexibility of moving around a `.vimrc` file to set up a new editor.

I used [amix's .vimrc](https://github.com/amix/vimrc) as a starting point. And removed most of the configuration that didn't make sense to me -- my goal was to end with something simple.

I also added the following plugins:
*  gruvbox theme
*  vim-fugitive
*  vim-go
*  nerd-tree

Similar to the .vimrc, I tried to keep my plugins simple.

# Blog
I wanted an easy way to keep track of thoughts and notes. I'll mostly use this as a project log for myself. But hope to also have some more polished content eventually.

Used github pages and jekyll making the blog easy to set up and maintain. I registered a domain with Google Domain.

# Future plans
A few things I hope to eventually figure out:
*  get ssl working with github pages
* get ssl working for some sort of dev instance
*  ...

