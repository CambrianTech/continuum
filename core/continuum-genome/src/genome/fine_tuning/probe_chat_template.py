# Probe whether a HF base's tokenizer carries a chat template — the fact that
# decides which dataset schema mlx_lm.lora can train on. {"prompt","completion"}
# rows are rendered THROUGH tokenizer.apply_chat_template; a template-less
# tokenizer (Devstral/Tekken ships its template outside tokenizer_config) makes
# mlx_lm.lora exit 1 ("Cannot use chat template functions"), which killed the
# first lived-curriculum train (recall-trust, 2026-07-10). Template-less bases
# train on the raw {"text"} schema instead. Run under the SAME interpreter that
# runs mlx_lm.lora so the answer matches what training will actually see.
import sys

from transformers import AutoTokenizer

tokenizer = AutoTokenizer.from_pretrained(sys.argv[1])
print("yes" if getattr(tokenizer, "chat_template", None) else "no")
