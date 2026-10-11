#!/bin/sh
# Deterministic SGR color demo for tests/terminal_rendering.rs.
# 16-color, 256-color, and truecolor foreground/background plus attributes.
# No dates, no randomness, no network, no host data.
ESC=$(printf '\033')
printf '%s' "${ESC}[2J${ESC}[1;1H"
printf '%s' "${ESC}[1;31mR${ESC}[0m"
printf '%s' "${ESC}[38;5;196mA${ESC}[0m"
printf '%s' "${ESC}[48;5;21mB${ESC}[0m"
printf '%s' "${ESC}[38;2;10;20;30mC${ESC}[0m"
printf '%s' "${ESC}[48;2;200;150;100mD${ESC}[0m"
printf '%s' "${ESC}[3;4mE${ESC}[0m"
printf '%s' "${ESC}[7mF${ESC}[0m"
printf '%s' "${ESC}[2;32mG${ESC}[0m"
