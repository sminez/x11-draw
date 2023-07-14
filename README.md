# README

This is a bare bones text-based graphics library intended for use with the `penrose` window
manager library for rendering the status bar. The current state of this repo is a proof of
concept for the API before integrating with the existing `penrose_ui` crate.

This crate is a thin wrapper around xlib, xft and fontconfig.

> To see the crate in use you can run one of the examples provided.


### Still to do

- documentation
- safety comments around the use of C FFI for calling out to xlib, xft and fontconfig

> This repo will not be published to crates.io as a crate. It is going to be inlined into
> the existing `penrose_ui` crate and is provided here as a stand alone example of a
> minimal graphics layer.
