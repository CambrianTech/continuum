#!/usr/bin/env python3
"""Generate training JSONL from RealClassEval challenges with reference implementations.

Creates one training example per challenge:
  user: skeleton + tests → assistant: reference implementation

Usage: python3 scripts/generate-remediation-jsonl.py <dataset_dir> <session_id>
Output: writes remediation-<session_id[:8]>.jsonl to dataset_dir, prints JSON result
"""

import json
import sys
from pathlib import Path


def main():
    if len(sys.argv) < 3:
        print('Usage: generate-remediation-jsonl.py <dataset_dir> <session_id>', file=sys.stderr)
        sys.exit(1)

    dataset_dir = Path(sys.argv[1])
    session_id = sys.argv[2]
    challenges_path = dataset_dir / 'challenges.json'

    if not challenges_path.exists():
        print(json.dumps({"error": f"challenges.json not found at {challenges_path}"}))
        sys.exit(1)

    challenges = json.load(open(challenges_path))['challenges']
    out_path = dataset_dir / f'remediation-{session_id[:8]}.jsonl'

    count = 0
    with open(out_path, 'w') as f:
        for c in challenges:
            ref = c.get('referenceImpl', '')
            if not ref:
                continue
            example = {"messages": [
                {"role": "user", "content": (
                    f"Implement the following Python class.\n\n"
                    f"Skeleton:\n{c['skeleton']}\n\n"
                    f"Tests:\n{c['testCode']}\n\n"
                    f"Output ONLY the implementation."
                )},
                {"role": "assistant", "content": ref}
            ]}
            f.write(json.dumps(example) + '\n')
            count += 1

    print(json.dumps({"datasetPath": str(out_path), "exampleCount": count}))


if __name__ == '__main__':
    main()
