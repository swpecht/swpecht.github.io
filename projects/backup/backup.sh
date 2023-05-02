#!/bin/bash
source /etc/restic-env
restic backup ~/src/ --exclude **/target
