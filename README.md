<h1 align="center">
  <img src="docs/httprs.png" alt="httprs">
  <br>
</h1>

`httprs` is a fast toolkit for probing hosts from stdin to stdout.
Probe hosts using custom regular expressions.

![preview](docs/preview.gif)

## Requirements

* OpenSSL with headers

## Usage

```
🧨 http toolkit that allows probing many hosts.

Usage: httprs [OPTIONS]

Options:
  -h, --help     Print help
  -V, --version  Print version

Optimizations ⚙️:
  -T, --timeout <TIMEOUT>  request duration threshold in milliseconds [default: 6000]

Rate-Limit 🐌:
  -t, --tasks <TASKS>  number of concurrent requests [default: 60]

Matchers 🔍:
  -r, --match-regex <MATCH_REGEXES_PATH>  path to a list of regex patterns
```

```shell
echo google.fr | ./httprs
```

> https://google.fr
