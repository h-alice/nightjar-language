// Copyright 2026 Wayne Hong (h-alice) <contact@halice.art>
// Nightjar Language Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Boolean operation implementations.

pub fn apply_and(left: bool, right: bool) -> bool {
    left && right
}

pub fn apply_or(left: bool, right: bool) -> bool {
    left || right
}

pub fn apply_not(value: bool) -> bool {
    !value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn and_truth_table() {
        assert!(!apply_and(false, false));
        assert!(!apply_and(false, true));
        assert!(!apply_and(true, false));
        assert!(apply_and(true, true));
    }

    #[test]
    fn or_truth_table() {
        assert!(!apply_or(false, false));
        assert!(apply_or(false, true));
        assert!(apply_or(true, false));
        assert!(apply_or(true, true));
    }

    #[test]
    fn not_inverts() {
        assert!(apply_not(false));
        assert!(!apply_not(true));
    }
}
