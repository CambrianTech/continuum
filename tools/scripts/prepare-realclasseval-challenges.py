#!/usr/bin/env python3
"""Pre-process RealClassEval eval.jsonl into structured challenges JSON.

Extracts skeleton, test code, module name, and reference implementation
from each example — no LLM needed for curriculum selection.

Usage: python3 scripts/prepare-realclasseval-challenges.py <dataset_dir> [limit]
Output: writes challenges.json to dataset_dir
"""

import json
import re
import sys
from pathlib import Path


def extract_challenge(example: dict, index: int) -> dict:
    """Extract structured challenge from a raw JSONL example."""
    user_msg = example['messages'][0]['content']
    assistant_msg = example['messages'][1]['content']

    # Split on ```python to find code blocks
    blocks = user_msg.split('```python')

    # Skeleton = first code block
    skeleton = blocks[1].split('```')[0].strip() if len(blocks) > 1 else ''

    # Test code = second code block
    test_code = blocks[2].split('```')[0].strip() if len(blocks) > 2 else ''

    # Module name from test import (e.g., "import snippet_191 as module_0")
    mod_match = re.search(r'import (snippet_\d+)', test_code)
    module_name = mod_match.group(1) if mod_match else f'solution_{index}'

    # Class name from skeleton
    class_match = re.search(r'class (\w+)', skeleton)
    class_name = class_match.group(1) if class_match else f'Unknown_{index}'

    # Reference implementation (strip markdown fences)
    ref = assistant_msg.strip()
    if ref.startswith('```'):
        ref = ref.split('\n', 1)[1].rsplit('```', 1)[0].strip()

    return {
        'index': index,
        'className': class_name,
        'moduleName': module_name,
        'skeleton': skeleton,
        'testCode': test_code,
        'referenceImpl': ref,
    }


def main():
    if len(sys.argv) < 2:
        print('Usage: prepare-realclasseval-challenges.py <dataset_dir> [limit]', file=sys.stderr)
        sys.exit(1)

    dataset_dir = Path(sys.argv[1])
    limit = int(sys.argv[2]) if len(sys.argv) > 2 else None
    eval_path = dataset_dir / 'eval.jsonl'

    if not eval_path.exists():
        print(f'Error: {eval_path} not found', file=sys.stderr)
        sys.exit(1)

    challenges = []
    with open(eval_path) as f:
        for i, line in enumerate(f):
            if limit and i >= limit:
                break
            example = json.loads(line.strip())
            challenge = extract_challenge(example, i)
            challenges.append(challenge)

    output = {
        'totalChallenges': len(challenges),
        'challenges': challenges,
    }

    out_path = dataset_dir / 'challenges.json'
    with open(out_path, 'w') as f:
        json.dump(output, f, indent=2)

    print(json.dumps({
        'success': True,
        'totalChallenges': len(challenges),
        'outputPath': str(out_path),
    }))


if __name__ == '__main__':
    main()
