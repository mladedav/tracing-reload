# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/mladedav/tracing-reload/compare/tracing-reload-v0.1.0...tracing-reload-v0.1.1) - 2026-08-03

### Added

- track whether a layer downcasted and cleanup non-downcasted layers

### Other

- use `Arc` for layers instead of `Vec` indices
- remove unneeded `Pin`
- document all error cases
- add missing safety comment
- add badges to readme
- expand MSRV section in README
- remove cargo update from cargo-deny check

