% remote-app-headless(1) | Remote App CLI

# NAME

remote-app-headless - inspect and maintain the encrypted Remote App session store

# SYNOPSIS

`remote-app-headless` `list-sessions`

`remote-app-headless` `export-sessions` *FILE*

`remote-app-headless` `import-sessions` *FILE*

`remote-app-headless` `reset-config`

`remote-app-headless` `completions` *SHELL*

# SECURITY

Password prompts do not echo. Exported files remain encrypted. Treat paths and
backups as sensitive metadata even though session records are authenticated.
