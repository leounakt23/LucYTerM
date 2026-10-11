#!/bin/sh
# Deterministic VT100 cursor-addressing and erase demo for
# tests/terminal_rendering.rs. No dates, no randomness, no network.
ESC=$(printf '\033')
printf '%s' "${ESC}[2J"
printf '%s' "${ESC}[4;1HBOT"
printf '%s' "${ESC}[1;1HTOP"
printf '%s' "${ESC}[2;1HXX"
printf '%s' "${ESC}[2;2H"
printf '%s' "${ESC}[K"
