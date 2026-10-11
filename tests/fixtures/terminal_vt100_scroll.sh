#!/bin/sh
# Deterministic scroll-overflow demo for tests/terminal_rendering.rs.
# Six CRLF-terminated lines into a four-row terminal; the top must
# scroll into history. No dates, no randomness, no network.
printf 'S0\r\nS1\r\nS2\r\nS3\r\nS4\r\nS5\r\n'
