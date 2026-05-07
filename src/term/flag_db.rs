/// Minimal flag-description database for common shell commands.
/// Returns a static description string, or `None` if the flag/command is unknown.
pub fn lookup_flag(cmd: &str, flag: &str) -> Option<&'static str> {
    match cmd {
        "ls" => ls(flag),
        "git" => git(flag),
        "cargo" => cargo(flag),
        "grep" | "rg" => grep(flag),
        "docker" => docker(flag),
        "kubectl" | "k" => kubectl(flag),
        "ssh" => ssh(flag),
        "curl" => curl(flag),
        "tar" => tar(flag),
        "find" => find(flag),
        _ => None,
    }
}

fn ls(flag: &str) -> Option<&'static str> {
    match flag {
        "-l" => Some("long listing format"),
        "-a" | "--all" => Some("include entries starting with ."),
        "-h" | "--human-readable" => Some("human-readable sizes"),
        "-r" | "--reverse" => Some("reverse sort order"),
        "-t" => Some("sort by modification time"),
        "-S" => Some("sort by file size"),
        "-R" | "--recursive" => Some("list subdirectories recursively"),
        "-1" => Some("one file per line"),
        "-G" | "--color" => Some("colorize output"),
        "-d" | "--directory" => Some("list directory itself, not contents"),
        "-n" | "--numeric-uid-gid" => Some("print numeric user and group IDs"),
        _ => None,
    }
}

fn git(flag: &str) -> Option<&'static str> {
    match flag {
        "--help" => Some("show help for the command"),
        "--version" => Some("print git version"),
        "--no-pager" => Some("do not pipe output into a pager"),
        "--global" => Some("read/write from global ~/.gitconfig"),
        "--local" => Some("read/write from repo .git/config"),
        "--system" => Some("read/write from system gitconfig"),
        "-v" | "--verbose" => Some("be more verbose"),
        "-q" | "--quiet" => Some("suppress output"),
        "-p" | "--patch" => Some("interactively choose hunks to stage"),
        "-n" | "--dry-run" => Some("show what would be done without doing it"),
        "-m" => Some("use the given message as the commit message"),
        "-a" | "--all" => Some("stage all tracked changes before committing"),
        "--amend" => Some("replace the tip of the current branch"),
        "--no-edit" => Some("reuse the previous commit message"),
        "-f" | "--force" => Some("force the operation"),
        "--soft" => Some("reset HEAD, keep index and working tree"),
        "--mixed" => Some("reset HEAD and index, keep working tree"),
        "--hard" => Some("reset HEAD, index and working tree"),
        "--oneline" => Some("condense each commit to one line"),
        "--graph" => Some("draw ASCII graph of the branch structure"),
        "--stat" => Some("show file-change statistics"),
        "--cached" | "--staged" => Some("diff between index and last commit"),
        "--no-ff" => Some("create a merge commit even if fast-forward is possible"),
        "--rebase" => Some("rebase instead of merge"),
        "--squash" => Some("squash commits into one without a merge commit"),
        "--depth" => Some("create a shallow clone with N commits"),
        "-u" | "--set-upstream" => Some("set upstream tracking branch"),
        "--tags" => Some("fetch / push all tags"),
        "--prune" => Some("remove remote-tracking refs that no longer exist"),
        _ => None,
    }
}

fn cargo(flag: &str) -> Option<&'static str> {
    match flag {
        "--release" => Some("build in release mode with optimizations"),
        "-q" | "--quiet" => Some("suppress output"),
        "-v" | "--verbose" => Some("verbose output; use twice for very verbose"),
        "--all-features" => Some("activate all available features"),
        "--no-default-features" => Some("do not activate the default features"),
        "--features" => Some("space or comma-separated list of features"),
        "-p" | "--package" => Some("package to operate on"),
        "--workspace" => Some("operate on all packages in the workspace"),
        "--message-format" => Some("output format: human, json, short"),
        "--target" => Some("target triple (e.g. x86_64-unknown-linux-musl)"),
        "--manifest-path" => Some("path to Cargo.toml"),
        "-j" | "--jobs" => Some("number of parallel compilation jobs"),
        "--nocapture" => Some("do not capture test stdout/stderr"),
        "--lib" => Some("only the library target"),
        "--bin" => Some("only the specified binary"),
        "--example" => Some("only the specified example"),
        "--test" => Some("only the specified integration test"),
        "--bench" => Some("only the specified benchmark"),
        "--color" => Some("coloring: auto, always, never"),
        "--frozen" => Some("require Cargo.lock to be up-to-date"),
        "--locked" => Some("assert Cargo.lock unchanged"),
        "--offline" => Some("run without network access"),
        _ => None,
    }
}

fn grep(flag: &str) -> Option<&'static str> {
    match flag {
        "-r" | "--recursive" => Some("search files recursively"),
        "-i" | "--ignore-case" => Some("case-insensitive matching"),
        "-n" | "--line-number" => Some("prefix output with line number"),
        "-v" | "--invert-match" => Some("select non-matching lines"),
        "-l" | "--files-with-matches" => Some("print only filenames with matches"),
        "-c" | "--count" => Some("print count of matching lines per file"),
        "-e" | "--regexp" => Some("use PATTERN as the regexp"),
        "-E" | "--extended-regexp" => Some("use extended regular expressions"),
        "-F" | "--fixed-strings" => Some("treat pattern as a literal string"),
        "-w" | "--word-regexp" => Some("match whole words only"),
        "-A" => Some("print NUM lines after each match"),
        "-B" => Some("print NUM lines before each match"),
        "-C" => Some("print NUM lines around each match"),
        "--color" | "--colour" => Some("highlight matching strings"),
        "-o" | "--only-matching" => Some("print only the matched part"),
        "-q" | "--quiet" => Some("suppress all output"),
        "-s" | "--no-messages" => Some("suppress error messages"),
        "-m" | "--max-count" => Some("stop after NUM matches"),
        _ => None,
    }
}

fn docker(flag: &str) -> Option<&'static str> {
    match flag {
        "-d" | "--detach" => Some("run container in background"),
        "-p" | "--publish" => Some("publish a container port to the host"),
        "-v" | "--volume" => Some("bind mount a volume"),
        "-e" | "--env" => Some("set an environment variable"),
        "--rm" => Some("automatically remove the container on exit"),
        "--name" => Some("assign a name to the container"),
        "-i" | "--interactive" => Some("keep STDIN open even if not attached"),
        "-t" | "--tty" => Some("allocate a pseudo-TTY"),
        "--network" => Some("connect the container to a network"),
        "--entrypoint" => Some("override the default entrypoint"),
        "--no-cache" => Some("do not use cache when building"),
        "-f" | "--file" => Some("path to the Dockerfile"),
        "--tag" => Some("name and optional tag (name:tag)"),
        "--platform" => Some("set target platform (e.g. linux/amd64)"),
        "-q" | "--quiet" => Some("suppress verbose output"),
        "--pull" => Some("always attempt to pull newer image"),
        "--restart" => Some("restart policy: no, on-failure, always, unless-stopped"),
        "--memory" | "-m" => Some("memory limit (e.g. 512m, 2g)"),
        "--cpus" => Some("number of CPUs"),
        _ => None,
    }
}

fn kubectl(flag: &str) -> Option<&'static str> {
    match flag {
        "-n" | "--namespace" => Some("Kubernetes namespace to use"),
        "-A" | "--all-namespaces" => Some("across all namespaces"),
        "-o" | "--output" => Some("output format: json, yaml, wide, name, jsonpath"),
        "--dry-run" => Some("client | server | none — simulate the request"),
        "--context" => Some("kubeconfig context to use"),
        "--kubeconfig" => Some("path to kubeconfig file"),
        "-l" | "--selector" => Some("label selector (e.g. app=nginx)"),
        "--field-selector" => Some("field selector (e.g. status.phase=Running)"),
        "-f" | "--filename" => Some("filename, directory, or URL to resource file"),
        "-w" | "--watch" => Some("watch for changes after listing"),
        "--force" => Some("immediately remove resources from API"),
        "--grace-period" => Some("seconds before forceful termination (0=immediate)"),
        "--cascade" => Some("cascade deletion to dependent resources"),
        "-R" | "--recursive" => Some("process the directory used in -f recursively"),
        "--overwrite" => Some("allow overwriting existing labels/annotations"),
        "--record" => Some("record the command in the resource annotation"),
        _ => None,
    }
}

fn ssh(flag: &str) -> Option<&'static str> {
    match flag {
        "-p" => Some("port to connect to on the remote host"),
        "-i" => Some("identity file (private key)"),
        "-L" => Some("local port forwarding (local:host:remote)"),
        "-R" => Some("remote port forwarding"),
        "-D" => Some("dynamic SOCKS proxy port"),
        "-N" => Some("do not execute a remote command"),
        "-f" => Some("go to background before command execution"),
        "-v" => Some("verbose mode (use -vvv for more)"),
        "-q" | "--quiet" => Some("quiet mode"),
        "-C" => Some("enable compression"),
        "-X" => Some("enable X11 forwarding"),
        "-A" => Some("enable agent forwarding"),
        "-T" => Some("disable pseudo-terminal allocation"),
        "-o" => Some("option in format used in ssh_config"),
        "-J" => Some("jump through a proxy host (ProxyJump)"),
        _ => None,
    }
}

fn curl(flag: &str) -> Option<&'static str> {
    match flag {
        "-o" | "--output" => Some("write output to file instead of stdout"),
        "-O" | "--remote-name" => Some("save file with its remote name"),
        "-s" | "--silent" => Some("silent mode — no progress or error messages"),
        "-v" | "--verbose" => Some("make curl verbose during the operation"),
        "-I" | "--head" => Some("fetch headers only (HEAD request)"),
        "-X" | "--request" => Some("HTTP method: GET, POST, PUT, DELETE, PATCH"),
        "-d" | "--data" => Some("HTTP POST data"),
        "-H" | "--header" => Some("extra header to include in the request"),
        "-u" | "--user" => Some("server user and password (user:password)"),
        "-L" | "--location" => Some("follow redirects"),
        "-k" | "--insecure" => Some("allow insecure TLS connections"),
        "--compressed" => Some("request compressed response"),
        "-c" | "--cookie-jar" => Some("save cookies to file"),
        "-b" | "--cookie" => Some("send cookies from string or file"),
        "-A" | "--user-agent" => Some("send User-Agent header"),
        "--max-time" => Some("maximum time in seconds for the transfer"),
        "--retry" => Some("retry request NUM times on transient error"),
        "-F" | "--form" => Some("multipart/form-data POST field"),
        _ => None,
    }
}

fn tar(flag: &str) -> Option<&'static str> {
    match flag {
        "-c" | "--create" => Some("create a new archive"),
        "-x" | "--extract" => Some("extract files from an archive"),
        "-t" | "--list" => Some("list the contents of an archive"),
        "-v" | "--verbose" => Some("list files processed"),
        "-f" | "--file" => Some("use ARCHIVE file (required)"),
        "-z" | "--gzip" => Some("compress/decompress with gzip (.tar.gz)"),
        "-j" | "--bzip2" => Some("compress/decompress with bzip2 (.tar.bz2)"),
        "-J" | "--xz" => Some("compress/decompress with xz (.tar.xz)"),
        "-C" | "--directory" => Some("change to DIRECTORY before operations"),
        "-p" | "--preserve-permissions" => Some("preserve file permissions"),
        "--exclude" => Some("exclude files matching PATTERN"),
        "-r" | "--append" => Some("append files to an existing archive"),
        "-u" | "--update" => Some("append files newer than copy in archive"),
        _ => None,
    }
}

fn find(flag: &str) -> Option<&'static str> {
    match flag {
        "-name" => Some("search by filename pattern"),
        "-iname" => Some("search by filename (case-insensitive)"),
        "-type" => Some("file type: f=file, d=dir, l=symlink"),
        "-size" => Some("search by size (e.g. +1M, -10k)"),
        "-mtime" => Some("modified N*24h ago (+N = older, -N = newer)"),
        "-newer" => Some("files newer than the given reference file"),
        "-maxdepth" => Some("descend at most N directory levels"),
        "-mindepth" => Some("do not apply tests at levels less than N"),
        "-not" | "!" => Some("negate the following expression"),
        "-exec" => Some("execute command on each found file"),
        "-print" => Some("print full path of found files"),
        "-print0" => Some("print path followed by NUL (safe for xargs -0)"),
        "-delete" => Some("delete found files"),
        "-empty" => Some("match empty files or directories"),
        "-regex" => Some("match path against POSIX extended regex"),
        _ => None,
    }
}
