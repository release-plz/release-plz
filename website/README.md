# Website

This website is built using [Docusaurus 2](https://docusaurus.io/).

## Local Development

```sh
npm i && npm start
```

This command starts a local development server and opens up a browser window. Most changes are
reflected live without having to restart the server.

## Homepage video

The homepage video is served in two formats:
AV1 for browsers that support it, and H.264 for the others (e.g. most Safari versions).
To update the video, re-encode it with:

```sh
ffmpeg -i video.mp4 -an -c:v libaom-av1 -crf 38 -b:v 0 -cpu-used 4 -row-mt 1 -tiles 2x2 -pix_fmt yuv420p -movflags +faststart static/video/release-plz-av1.mp4
ffmpeg -i video.mp4 -an -c:v libx264 -preset veryslow -tune animation -crf 28 -pix_fmt yuv420p -profile:v high -level:v 4.0 -movflags +faststart static/video/release-plz-h264.mp4
ffmpeg -i video.mp4 -frames:v 1 -c:v libwebp -quality 75 -compression_level 6 static/img/release-plz-video-poster.webp
```

Then update the transcript in `src/pages/index.tsx`.
