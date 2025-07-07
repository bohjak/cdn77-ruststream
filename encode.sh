#!/bin/bash

set -e

BASE='http://127.0.0.1:3000'
PATH='foo/bar/main.mpd'
FILE='test.mp4'

function encode {
	/usr/bin/ffmpeg \
		-loglevel verbose \
		\
		-re \
		-f lavfi \
		-i pal100bars=size=1280x720:rate=25 \
		\
		-vf "drawtext='text=T %{localtime\:%X.%N}\ | F %{frame_num}:\
		fontsize=72:\
		fontcolor=black:\
		box=1:\
		boxcolor=white:\
		font=monospace:\
		y=324:x=50:\
		boxborderw=10:\
		bordercolor=white'" \
		\
		-c:v libx264 \
		-preset ultrafast \
		-profile baseline \
		-b:v 10M \
		\
		-dash_segment_type mp4 \
		-ldash true \
		-method PUT \
		-window_size 20 \
		-streaming true \
		-seg_duration 15 \
		-remove_at_exit true \
		-target_latency 1s \
		-use_timeline false \
		-frag_type every_frame \
		-utc_timing_url 'http://time.akamai.com?iso&amp;ms' \
		-format_options 'movflags=cmaf' \
		-timeout 0.5 \
		-write_prft 1 \
		\
		"$BASE/$PATH"
}

function main {
	encode
}

main "$@"
