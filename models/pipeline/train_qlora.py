#!/usr/bin/env python3
"""qLoRA fine-tuning for the three WorldSimulator models.

Each model is trained with 4-bit base + LoRA adapters. The causal model
(mustafakemal) and data model (inalcik) use SFT on the chat dataset; the
retrieval model (ortayli) is trained with a contrastive objective on the
corpus.

Outputs adapter dirs under models/adapters/{mustafakemal,inalcik,ortayli}.
Merge & GGUF export: see merge_lora.py.

Usage:
  python3 models/pipeline/train_qlora.py --model mustafakemal
  python3 models/pipeline/train_qlora.py --model inalcik
  python3 models/pipeline/train_qlora.py --model ortayli
"""

import argparse
import json
import os

import torch
from datasets import load_dataset
from transformers import (
    AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig,
    AutoModelForSequenceClassification, TrainingArguments, Trainer,
)
from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
from trl import SFTTrainer

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = os.path.join(HERE, "..", "base")
ADAPTERS = os.path.join(HERE, "..", "adapters")
DATA = os.path.join(HERE, "data")

CONFIG = {
    "mustafakemal": {
        "base": os.path.join(BASE, "qwen3-8b"),
        "out": os.path.join(ADAPTERS, "mustafakemal"),
        "max_seq": 4096, "epochs": 3, "lr": 2e-4, "r": 32, "alpha": 64,
    },
    "inalcik": {
        "base": os.path.join(BASE, "qwen25-3b"),
        "out": os.path.join(ADAPTERS, "inalcik"),
        "max_seq": 2048, "epochs": 3, "lr": 2e-4, "r": 32, "alpha": 64,
    },
    "ortayli": {
        "base": os.path.join(BASE, "qwen3-embed-0.6b"),
        "out": os.path.join(ADAPTERS, "ortayli"),
        "max_seq": 1024, "epochs": 2, "lr": 1e-4, "r": 16, "alpha": 32,
    },
}


def train_causal(name):
    cfg = CONFIG[name]
    tok = AutoTokenizer.from_pretrained(cfg["base"], trust_remote_code=True)
    tok.pad_token = tok.eos_token

    bnb = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.bfloat16)
    model = AutoModelForCausalLM.from_pretrained(
        cfg["base"], quantization_config=bnb, device_map="auto", trust_remote_code=True
    )
    model = prepare_model_for_kbit_training(model)

    lora = LoraConfig(
        r=cfg["r"], lora_alpha=cfg["alpha"], lora_dropout=0.05,
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
        bias="none", task_type="CAUSAL_LM",
    )
    model = get_peft_model(model, lora)

    ds = load_dataset("json", data_files={
        "train": os.path.join(DATA, "train.jsonl"),
        "validation": os.path.join(DATA, "val.jsonl"),
    })

    args = TrainingArguments(
        output_dir=cfg["out"], per_device_train_batch_size=1,
        gradient_accumulation_steps=8, num_train_epochs=cfg["epochs"],
        learning_rate=cfg["lr"], bf16=True, logging_steps=10,
        save_strategy="epoch", report_to="none",
        max_seq_length=cfg["max_seq"],
    )
    trainer = SFTTrainer(
        model=model, tokenizer=tok, train_dataset=ds["train"],
        eval_dataset=ds["validation"], args=args,
        max_seq_length=cfg["max_seq"],
    )
    trainer.train()
    model.save_pretrained(cfg["out"])
    tok.save_pretrained(cfg["out"])
    print(f"saved {name} adapter -> {cfg['out']}")


def train_embedding():
    """Contrastive fine-tuning of the embedding model on the corpus."""
    cfg = CONFIG["ortayli"]
    tok = AutoTokenizer.from_pretrained(cfg["base"], trust_remote_code=True)
    bnb = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.bfloat16)
    model = AutoModelForSequenceClassification.from_pretrained(
        cfg["base"], quantization_config=bnb, num_labels=1, trust_remote_code=True
    ).to("cuda" if torch.cuda.is_available() else "cpu")
    model = prepare_model_for_kbit_training(model)
    lora = LoraConfig(
        r=cfg["r"], lora_alpha=cfg["alpha"], lora_dropout=0.05,
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
        task_type="SEQ_CLS",
    )
    model = get_peft_model(model, lora)

    # Build (query, positive, negative) triplets from corpus sentences.
    corpus = [json.loads(l)["text"] for l in open(os.path.join(DATA, "corpus.jsonl"))]
    triplets = []
    import random
    rng = random.Random(1)
    for i in range(0, len(corpus) - 1, 2):
        triplets.append({"q": corpus[i], "p": corpus[i + 1], "n": rng.choice(corpus)})

    def tok_fn(x):
        enc = tok(x["q"], x["p"], truncation=True, padding="max_length", max_length=cfg["max_seq"], return_tensors="pt")
        neg = tok(x["n"], truncation=True, padding="max_length", max_length=cfg["max_seq"], return_tensors="pt")
        return {
            "input_ids": torch.cat([enc["input_ids"], neg["input_ids"]]),
            "attention_mask": torch.cat([enc["attention_mask"], neg["attention_mask"]]),
            "labels": torch.tensor([1.0, 0.0]),
        }

    class TripletDataset(torch.utils.data.Dataset):
        def __init__(self, items):
            self.items = [tok_fn(t) for t in items]

        def __len__(self):
            return len(self.items)

        def __getitem__(self, i):
            return self.items[i]

    ds = TripletDataset(triplets)

    def contrastive_loss(m, inputs):
        out = m(input_ids=inputs["input_ids"], attention_mask=inputs["attention_mask"]).logits.view(-1)
        pos, neg = out[0], out[1]
        return torch.relu(0.4 - (pos - neg)).mean()

    args = TrainingArguments(
        output_dir=cfg["out"], per_device_train_batch_size=2,
        num_train_epochs=cfg["epochs"], learning_rate=cfg["lr"],
        bf16=True, logging_steps=10, save_strategy="epoch", report_to="none",
    )
    trainer = Trainer(model=model, args=args, train_dataset=ds, data_collator=lambda b: {
        "input_ids": torch.stack([x["input_ids"] for x in b]),
        "attention_mask": torch.stack([x["attention_mask"] for x in b]),
        "labels": torch.stack([x["labels"] for x in b]),
    })
    trainer.train()
    model.save_pretrained(cfg["out"])
    tok.save_pretrained(cfg["out"])
    print(f"saved ortayli adapter -> {cfg['out']}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True, choices=["mustafakemal", "inalcik", "ortayli"])
    args = ap.parse_args()
    os.makedirs(ADAPTERS, exist_ok=True)
    if args.model == "ortayli":
        train_embedding()
    else:
        train_causal(args.model)


if __name__ == "__main__":
    main()
