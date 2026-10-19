#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo install rulewright --version '=0.1.2' --locked
cargo install markupwright --version '=0.1.3' --locked
