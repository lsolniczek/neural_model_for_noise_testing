#!/usr/bin/env python3
"""Train a surrogate MLP for the neural preset optimizer (Priority 14b).

Reads a CSV of (genome, goal, brain_type, config_flags, score) rows generated
by the `generate-data` CLI command, trains a small MLP, and exports the weights
as a flat f32 little-endian binary file that the Rust `SurrogateModel` can load.

Usage:
    python tools/train_surrogate.py training_data.csv surrogate_weights.bin

Requirements:
    pip install torch numpy pandas

The script is OFFLINE — it runs once, produces a weights file, and is never
called by the Rust binary at runtime. The weights file is the only artifact.
"""

import argparse
import struct
import sys
from pathlib import Path

import numpy as np
import pandas as pd
import torch
import torch.nn as nn
from torch.utils.data import DataLoader, TensorDataset

# ── Architecture (must match src/surrogate.rs) ──────────────────────────────

GENOME_DIM = 206
GOAL_DIM = 9
BRAIN_TYPE_DIM = 5
CONFIG_DIM = 4
INPUT_DIM = GENOME_DIM + GOAL_DIM + BRAIN_TYPE_DIM + CONFIG_DIM  # 208

DEFAULT_HIDDEN_DIMS = [256, 256, 128]
OUTPUT_DIM = 1


class SurrogateMLP(nn.Module):
    """MLP with configurable hidden dims. Default matches the Rust SurrogateModel
    architecture (208->256->256->128->1), but can be shrunk for small datasets.
    """

    def __init__(self, hidden_dims=None):
        super().__init__()
        if hidden_dims is None:
            hidden_dims = DEFAULT_HIDDEN_DIMS
        dims = [INPUT_DIM] + hidden_dims + [OUTPUT_DIM]
        layers = []
        for i in range(len(dims) - 1):
            layers.append(nn.Linear(dims[i], dims[i + 1]))
            if i < len(dims) - 2:
                layers.append(nn.ReLU())
            else:
                layers.append(nn.Sigmoid())
        self.net = nn.Sequential(*layers)

    def forward(self, x):
        return self.net(x).squeeze(-1)


# ── CSV parsing ─────────────────────────────────────────────────────────────

def load_csv(path: Path) -> tuple[np.ndarray, np.ndarray]:
    """Load training data CSV. Expected columns:
    g0, g1, ..., g189, goal_id, brain_type_id, assr, thalamic_gate, cet, phys_gate, score
    """
    df = pd.read_csv(path)
    n_rows = len(df)
    print(f"Loaded {n_rows} rows from {path}")

    # Genome columns: g0..g189, normalized to [0, 1] per-column.
    # The Rust SurrogateModel::build_input() normalizes using Preset::bounds();
    # here we approximate the same by normalizing to the data's own min/max
    # (which closely matches bounds since generate-data samples uniformly).
    genome_cols = [f"g{i}" for i in range(GENOME_DIM)]
    genomes_raw = df[genome_cols].values.astype(np.float64)
    g_min = genomes_raw.min(axis=0)
    g_max = genomes_raw.max(axis=0)
    g_range = g_max - g_min
    g_range[g_range < 1e-10] = 1.0  # avoid division by zero for constant columns
    genomes = ((genomes_raw - g_min) / g_range).astype(np.float32)
    print(f"  Genome range: min={genomes.min():.3f}, max={genomes.max():.3f} (normalized)")

    # Goal one-hot
    goal_ids = df["goal_id"].values.astype(int)
    goal_onehot = np.zeros((n_rows, GOAL_DIM), dtype=np.float32)
    for i, gid in enumerate(goal_ids):
        if 0 <= gid < GOAL_DIM:
            goal_onehot[i, gid] = 1.0

    # Brain type one-hot
    bt_ids = df["brain_type_id"].values.astype(int)
    bt_onehot = np.zeros((n_rows, BRAIN_TYPE_DIM), dtype=np.float32)
    for i, bid in enumerate(bt_ids):
        if 0 <= bid < BRAIN_TYPE_DIM:
            bt_onehot[i, bid] = 1.0

    # Config flags
    config_cols = ["assr", "thalamic_gate", "cet", "phys_gate"]
    configs = df[config_cols].values.astype(np.float32)

    # Combine into input matrix
    X = np.hstack([genomes, goal_onehot, bt_onehot, configs])
    assert X.shape[1] == INPUT_DIM, f"Input dim mismatch: {X.shape[1]} vs {INPUT_DIM}"

    y = df["score"].values.astype(np.float32)

    return X, y


# ── Weight export ───────────────────────────────────────────────────────────

def export_weights(model: SurrogateMLP, path: Path):
    """Export model weights to flat f32 little-endian binary.

    Format:
        Header: n_layers (u32), then (n_layers+1) dimension values (u32)
        Body:   for each layer: weights (row-major f32), then biases (f32)
    """
    # Extract Linear layers
    linears = [m for m in model.net if isinstance(m, nn.Linear)]
    n_layers = len(linears)

    dims = [linears[0].in_features]
    for layer in linears:
        dims.append(layer.out_features)

    with open(path, "wb") as f:
        # Header
        f.write(struct.pack("<I", n_layers))
        for d in dims:
            f.write(struct.pack("<I", d))

        # Layer data
        for layer in linears:
            # Weights: [out_features, in_features] row-major
            w = layer.weight.detach().cpu().numpy().astype(np.float32)
            f.write(w.tobytes())
            # Biases: [out_features]
            b = layer.bias.detach().cpu().numpy().astype(np.float32)
            f.write(b.tobytes())

    total_params = sum(p.numel() for p in model.parameters())
    file_size = path.stat().st_size
    print(f"Exported {n_layers} layers, {total_params:,} params, {file_size:,} bytes -> {path}")


# ── Training loop ───────────────────────────────────────────────────────────

def train(X: np.ndarray, y: np.ndarray, epochs: int = 200, lr: float = 1e-3,
          batch_size: int = 256, patience: int = 20, val_frac: float = 0.2,
          hidden_dims=None):
    """Train the surrogate MLP with early stopping on validation loss."""

    # Auto-size the architecture based on training data size.
    # Rule of thumb: total params should be ~1/5 of training samples.
    n_train_est = int(len(X) * (1 - val_frac))
    if hidden_dims is None:
        if n_train_est < 5000:
            hidden_dims = [64, 32]
        elif n_train_est < 20000:
            hidden_dims = [128, 64]
        else:
            hidden_dims = DEFAULT_HIDDEN_DIMS
        print(f"  Auto-selected hidden dims: {hidden_dims} (for {n_train_est} train samples)")

    # Train/val split
    n = len(X)
    n_val = int(n * val_frac)
    indices = np.random.permutation(n)
    val_idx, train_idx = indices[:n_val], indices[n_val:]

    X_train = torch.tensor(X[train_idx])
    y_train = torch.tensor(y[train_idx])
    X_val = torch.tensor(X[val_idx])
    y_val = torch.tensor(y[val_idx])

    train_ds = TensorDataset(X_train, y_train)
    train_dl = DataLoader(train_ds, batch_size=batch_size, shuffle=True)

    model = SurrogateMLP(hidden_dims=hidden_dims)
    optimizer = torch.optim.AdamW(model.parameters(), lr=lr, weight_decay=1e-4)
    loss_fn = nn.MSELoss()

    best_val_loss = float("inf")
    best_state = None
    stale = 0

    for epoch in range(epochs):
        # Train
        model.train()
        train_loss_sum = 0.0
        train_count = 0
        for xb, yb in train_dl:
            pred = model(xb)
            loss = loss_fn(pred, yb)
            optimizer.zero_grad()
            loss.backward()
            optimizer.step()
            train_loss_sum += loss.item() * len(xb)
            train_count += len(xb)

        # Validate
        model.eval()
        with torch.no_grad():
            val_pred = model(X_val)
            val_loss = loss_fn(val_pred, y_val).item()

        train_loss = train_loss_sum / train_count

        # R-squared
        ss_res = ((y_val.numpy() - val_pred.numpy()) ** 2).sum()
        ss_tot = ((y_val.numpy() - y_val.numpy().mean()) ** 2).sum()
        r2 = 1.0 - ss_res / ss_tot if ss_tot > 0 else 0.0

        if (epoch + 1) % 10 == 0 or epoch == 0:
            print(f"  Epoch {epoch+1:4d}  train_loss={train_loss:.6f}  "
                  f"val_loss={val_loss:.6f}  R2={r2:.4f}")

        # Early stopping
        if val_loss < best_val_loss - 1e-6:
            best_val_loss = val_loss
            best_state = {k: v.clone() for k, v in model.state_dict().items()}
            stale = 0
        else:
            stale += 1
            if stale >= patience:
                print(f"  Early stopping at epoch {epoch+1} (patience={patience})")
                break

    if best_state is not None:
        model.load_state_dict(best_state)

    # Final metrics
    model.eval()
    with torch.no_grad():
        val_pred = model(X_val)
        final_val_loss = loss_fn(val_pred, y_val).item()
        ss_res = ((y_val.numpy() - val_pred.numpy()) ** 2).sum()
        ss_tot = ((y_val.numpy() - y_val.numpy().mean()) ** 2).sum()
        final_r2 = 1.0 - ss_res / ss_tot if ss_tot > 0 else 0.0

    print(f"\n  Final val_loss={final_val_loss:.6f}  R2={final_r2:.4f}")
    print(f"  Train: {len(X_train)} samples, Val: {len(X_val)} samples")

    return model


# ── Main ────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Train surrogate MLP for preset optimizer")
    parser.add_argument("csv_path", type=Path, help="Training data CSV from generate-data command")
    parser.add_argument("output_path", type=Path, help="Output weights file (f32 binary)")
    parser.add_argument("--epochs", type=int, default=200, help="Max training epochs")
    parser.add_argument("--lr", type=float, default=1e-3, help="Learning rate")
    parser.add_argument("--batch-size", type=int, default=256, help="Batch size")
    parser.add_argument("--patience", type=int, default=20, help="Early stopping patience")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    parser.add_argument("--hidden", type=str, default=None,
                        help="Hidden layer dims, comma-separated (e.g. '128,64'). Auto-sized if omitted.")
    args = parser.parse_args()

    np.random.seed(args.seed)
    torch.manual_seed(args.seed)

    print(f"Loading data from {args.csv_path}...")
    X, y = load_csv(args.csv_path)

    hidden = None
    if args.hidden:
        hidden = [int(x) for x in args.hidden.split(",")]
    print(f"\nTraining surrogate MLP ({INPUT_DIM} -> {hidden or 'auto'} -> {OUTPUT_DIM})...")
    model = train(X, y, epochs=args.epochs, lr=args.lr,
                  batch_size=args.batch_size, patience=args.patience,
                  hidden_dims=hidden)

    print(f"\nExporting weights to {args.output_path}...")
    export_weights(model, args.output_path)
    print("Done.")


if __name__ == "__main__":
    main()
