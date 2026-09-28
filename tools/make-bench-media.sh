#!/usr/bin/env sh
# 1080p60 bench clips (20 s) into media/ (gitignored).
set -e
mkdir -p media
src="-f lavfi -i testsrc2=size=1920x1080:rate=60 -t 20"
ffmpeg -y -v error $src -c:v hap -format hap media/bench_hap1.mov
ffmpeg -y -v error $src -c:v hap -format hap_q media/bench_hapq.mov
ffmpeg -y -v error -f lavfi -i mandelbrot=size=1920x1080:rate=60 -t 20 -c:v hap -format hap_alpha media/bench_hap5.mov
ffmpeg -y -v error -f lavfi -i life=size=1920x1080:rate=60:mold=10 -t 20 -c:v hap -format hap media/bench_hap1b.mov
ffmpeg -y -v error $src -c:v h264_mf -b:v 12M -pix_fmt nv12 media/bench_h264.mp4
