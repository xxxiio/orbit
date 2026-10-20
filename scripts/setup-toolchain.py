#!/usr/bin/env python3
"""Optional developer/M15 toolchain setup. Never installs without explicit opt-in."""
import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import urllib.request

RUSTUP_URL = 'https://sh.rustup.rs'

def tools_ready():
    return all(shutil.which(x) for x in ('rustc','cargo','rustup'))

def refresh_path():
    cargo_bin = str(Path.home() / '.cargo' / 'bin')
    if cargo_bin not in os.environ.get('PATH', '').split(os.pathsep):
        os.environ['PATH'] = cargo_bin + os.pathsep + os.environ.get('PATH', '')

def main(argv=None):
    parser = argparse.ArgumentParser(description='Optional interactive Rust toolchain setup for Orbit contributors and M15 reviewers')
    group = parser.add_mutually_exclusive_group()
    group.add_argument('--yes', action='store_true', help='Explicitly authorize installing Rust via official rustup')
    group.add_argument('--no-install', action='store_true', help='Never install; just show requirements')
    args = parser.parse_args(argv)
    refresh_path()
    if tools_ready():
        print('Rust/Cargo/rustup are available.')
        return 0
    print('Rust is needed to BUILD or TEST Orbit from source, not to run a prebuilt Orbit application.')
    print('Official installer: https://rustup.rs/  (https://sh.rustup.rs)')
    print('The installer downloads and executes code; use only if you trust the official source.')
    if args.no_install:
        return 2
    if not args.yes:
        if not sys.stdin.isatty():
            print('Noninteractive session: no installation attempted. Run this setup command interactively or pass --yes.')
            return 2
        answer = input('Install Rust and Cargo for your user account now? [y/N] ').strip().lower()
        if answer not in ('y','yes'):
            print('Installation declined. You may continue with limited tests.')
            return 2
    system = platform.system()
    if system not in ('Linux','Darwin'):
        print('Automatic setup supports Linux/macOS only. For Windows, install rustup from https://rustup.rs/ and rerun.')
        return 2
    rustup_sh = Path.home() / '.cargo' / 'bin' / 'rustup'
    if not rustup_sh.exists():
        import tempfile
        with tempfile.TemporaryDirectory(prefix='orbit-rustup-') as tmp:
            installer = Path(tmp) / 'rustup-init.sh'
            print('Downloading the official rustup installation script over HTTPS...')
            try:
                with urllib.request.urlopen(RUSTUP_URL, timeout=30) as response:
                    installer.write_bytes(response.read(2_000_001))
                if installer.stat().st_size > 2_000_000:
                    raise ValueError('Rust installer exceeded the expected download limit')
                # Explicit confirmation was received above. No sudo / system package modification.
                result = subprocess.run(['sh', str(installer), '-y', '--no-modify-path'], check=False)
            except (OSError, ValueError) as exc:
                print(f'Failed to download installer: {exc}')
                return 1
            if result.returncode:
                print(f'Rust installer exited with status {result.returncode}.')
                return 1
    refresh_path()
    rustup = shutil.which('rustup')
    if not rustup:
        print('rustup not found after installation; please inspect ~/.cargo/bin.')
        return 1
    result = subprocess.run([rustup, 'component', 'add', 'rustfmt', 'clippy'], check=False)
    if result.returncode:
        print('Could not install rustfmt/clippy; retry: rustup component add rustfmt clippy')
        return 1
    if not (shutil.which('rustc') and shutil.which('cargo')):
        print('Rust commands still unavailable. Restart the shell or run: source "$HOME/.cargo/env"')
        return 1
    print('Rust toolchain ready. Run: ./scripts/m15-review.sh --profile full')
    return 0

if __name__ == '__main__':
    sys.exit(main())
