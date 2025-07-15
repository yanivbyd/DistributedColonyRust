#!/bin/bash
killall "QuickTime Player"
rm -rf output/colony_video.mp4
#cargo run -p frontend -- --video && open output/colony_video.mp4
cargo run -p frontend && open output/colony.png
