# `axctl`

A command line interface for [AXIS Communications](https://www.axis.com/en-us) devices.

# Installation

Install Rust. [`rustup`](https://rustup.rs) is a good choice. Then:

```console
$ cargo install axctl
```

# Usage

```
$ axctl
  axctl 0.1.0
  
  
  USAGE:
      axctl [FLAGS] <SUBCOMMAND>
  
  FLAGS:
      -h, --help       Prints help information
      -q, --quiet      Print less information
      -v, --verbose    Print more information
      -V, --version    Prints version information
  
  SUBCOMMANDS:
      help     Prints this message or the help of the given subcommand(s)
      log      Print the system log
      shell    Run an interactive shell on an AXIS camera
$
```

## Log

`axctl log` (a.k.a. `axctl tail`) shows the system log, colorizing the output. Like `tail`, it additionally supports
`-n` and `-f`:

```console
$ axctl log http://user:pass@172.16.4.30 -n 10 -f
2020-10-10T12:30:10.064-05:00 [ INFO    ] systemd[1] Condition check resulted in Temporary Directory (/tmp) being skipped.
2020-10-10T12:30:10.098-05:00 [ INFO    ] systemd[1] Starting Rotate log files...
2020-10-10T12:30:10.983-05:00 [ INFO    ] systemd[1] logrotate.service: Succeeded.
2020-10-10T12:30:11.024-05:00 [ INFO    ] systemd[1] Started Rotate log files.
2020-10-10T12:32:31.153-05:00 [ INFO    ] udhcpc[535] udhcpc: sending renew to 172.16.4.1
2020-10-10T12:32:33.199-05:00 [ INFO    ] udhcpc[535] udhcpc: lease of 172.16.4.30 obtained, lease time 600
2020-10-10T12:37:33.677-05:00 [ INFO    ] udhcpc[535] udhcpc: sending renew to 172.16.4.1
2020-10-10T12:37:35.716-05:00 [ INFO    ] udhcpc[535] udhcpc: lease of 172.16.4.30 obtained, lease time 600
2020-10-10T12:42:35.236-05:00 [ INFO    ] udhcpc[535] udhcpc: sending renew to 172.16.4.1
2020-10-10T12:42:37.268-05:00 [ INFO    ] udhcpc[535] udhcpc: lease of 172.16.4.30 obtained, lease time 600
^C
$
```

## Shell

`axctl shell` provides administrators with a root shell.

```console
$ axctl shell http://user:pass@172.16.3.233
 => connected to 172.16.3.233:54766 (session 5f602651-35ba-4534-aa61-c4d5ae432fc0)
sh: can't access tty; job control turned off
axis-00408cfb6888# head /proc/cpuinfo
processor	: 0
cpu		: CRIS
cpu revision	: 32
cpu model	: ARTPEC-3
cache size	: 32 KB
fpu		: no
mmu		: yes
mmu DMA bug	: no
ethernet	: 10/100 Mbps
token ring	: no
axis-00408cfb6888# exit

 => cleaning up session 5f602651-35ba-4534-aa61-c4d5ae432fc0
$
```

This function generates keypairs for mutual TLS, produces an application bundle, and uploads it to the device. It stops
short of actually installing the application -- the bundle fails validation -- but as a side effect it makes in-memory
changes to `/tmp` and runs a program listening for TLS connections on an arbitrary port. `axctl` connects, both sides
present a certificate, both sides verify the other, and ultimately your terminal connects to `sh`.
