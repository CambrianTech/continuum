# Probe whether a HF base's tokenizer carries a chat template — the fact that
# decides which dataset schema mlx_lm.lora can train on. {"prompt","completion"}
# rows are rendered THROUGH tokenizer.apply_chat_template; a template-less
# tokenizer (Devstral/Tekken ships its template outside tokenizer_config) makes
# mlx_lm.lora exit 1 ("Cannot use chat template functions"), which killed the
# first lived-curriculum train (recall-trust, 2026-07-10). Template-less bases
# train on the raw {"text"} schema instead. Run under the SAME interpreter that
# runs mlx_lm.lora so the answer matches what training will actually see.
# Its output is bounded by construction: the answer goes to a private copy of stdout, both
# standard streams go to the null device before transformers loads (nothing a library prints
# is captured), and a failure is one line of at most ~2 KiB on the answer channel.
import os
import sys

answer = os.fdopen(os.dup(1), "w")
null = os.open(os.devnull, os.O_WRONLY)
os.dup2(null, 1)
os.dup2(null, 2)
try:
    from transformers import AutoTokenizer

    tokenizer = AutoTokenizer.from_pretrained(sys.argv[1])
    answer.write("yes" if getattr(tokenizer, "chat_template", None) else "no")
except BaseException as error:  # the whole failure, said in one bounded line
    answer.write("error: " + repr(error)[-2000:])
    answer.flush()
    sys.exit(1)
answer.flush()
