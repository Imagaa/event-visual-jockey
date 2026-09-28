#!/usr/bin/env sh
# Regenerates the tiny test clips. Needs ffmpeg with the hap + h264_mf + prores_ks encoders (BtbN LGPL build has them).
set -e
out=crates/evj-media/tests/fixtures
mkdir -p "$out"
src="-f lavfi -i testsrc2=size=64x48:rate=30 -frames:v 3"
ffmpeg -y -v error $src -c:v hap -format hap -chunks 1 -compressor snappy "$out/hap1_64x48.mov"
ffmpeg -y -v error $src -c:v hap -format hap -chunks 4 -compressor snappy "$out/hap1_64x48_chunked.mov"
ffmpeg -y -v error $src -c:v hap -format hap -compressor none "$out/hap1_64x48_raw.mov"
ffmpeg -y -v error $src -c:v hap -format hap_alpha "$out/hap5_64x48.mov"
ffmpeg -y -v error $src -c:v hap -format hap_q "$out/hapq_64x48.mov"
ffmpeg -y -v error -f lavfi -i testsrc2=size=320x240:rate=30 -frames:v 30 -c:v h264_mf -pix_fmt nv12 "$out/h264_320x240.mp4"
ffmpeg -y -v error -i "$out/h264_320x240.mp4" -frames:v 1 -pix_fmt bgra -f rawvideo "$out/h264_320x240_f0.bgra"
ffmpeg -y -v error -f lavfi -i testsrc2=size=320x240:rate=30 -frames:v 5 -c:v prores_ks "$out/prores_320x240.mov"
head -c 4096 /dev/urandom > "$out/garbage.mov"
ffmpeg -y -v error -f lavfi -i color=c=red:s=64x36 -frames:v 1 "$out/red_64x36.png"
ffmpeg -y -v error -f lavfi -i color=c=blue:s=36x64 -frames:v 1 -q:v 2 "$out/blue_36x64.jpg"
ffmpeg -y -v error -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=1" -c:a libmp3lame -b:a 128k "$out/tone_44k_mono.mp3"
ffmpeg -y -v error -f lavfi -i testsrc2=size=320x240:rate=30 -f lavfi -i "sine=frequency=880:sample_rate=48000:duration=1" -ac 2 -frames:v 30 -c:v h264_mf -pix_fmt nv12 -c:a aac -shortest "$out/av_320x240.mp4"
