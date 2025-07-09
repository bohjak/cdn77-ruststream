# Chunked Store

## Features

Trie-backed in-memory file storage.

* **PUT {path}** - uploads body under the path, supports `Transfer-Encoding: chunked`
* **GET {path}** - streams the file at the path with `Transfer-Encoding: chunked` header set
* **DELETE {path}** - removes all references to the file at the path (active connections reading/writing to the file are unaffected)
* **LIST {prefix}** - list of all files matching the prefix
* **GET /** - HTML page with a list of all files
* **GET /player#{path}** - reference `dash.js` player pointed at the URL specified in the fragment (default `/stream/1/main.mpd`)

## Build

### Requirements:
* Edition 2024

```
cargo build
```
