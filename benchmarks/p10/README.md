# P-10 — benchmark wiarygodności i optymalizatora

P-10 ma dwa kolejne etapy. Pierwszy wybiera najkrótszy czas oceny i najmniejszy
panel seedów, które zachowują wynik 60 s/64 realizacji. Drugi używa wybranego
budżetu do porównania `legacy-v1` z wariantami `mixed-v2`. Wynik rozwojowy służy
do wyboru jednego kandydata, a osobna domena seedów potwierdzająca decyduje o
promocji.

## Uruchomienie

Z katalogu głównego repozytorium:

```bash
cargo build --release --locked --bins

target/release/p10_reliability_benchmark \
  --output-dir benchmarks/p10/reliability \
  --replicates 64 \
  --durations 3,6,10,12,20,30,60

python3 tools/run_p10_optimizer_benchmark.py \
  --binary target/release/neural_preset_optimizer \
  --reliability-manifest benchmarks/p10/reliability/manifest.json \
  --output-dir benchmarks/p10/optimizer
```

Drugi etap wykonuje domyślnie 1620 przebiegów rozwojowych i 720 przebiegów
potwierdzających. Każdy przebieg używa populacji 32, 31 generacji, ograniczeń
akustycznych, pięciu finalistów po 64 realizacje oraz niezależnej bezpośredniej
oceny 64-realizacyjnej. To kosztowny benchmark przeznaczony do długiego zadania
na maszynie benchmarkowej. Przerwany przebieg można wznowić przez `--resume`.

Samą analizę już zebranych wierszy uruchamia:

```bash
python3 tools/run_p10_optimizer_benchmark.py \
  --output-dir benchmarks/p10/optimizer \
  --analyze-only
```

## Artefakty

Etap wiarygodności zapisuje `raw_scores.csv`, `summary.csv` i `manifest.json`
z SHA-256 plików wejściowych do decyzji. Etap optymalizatora zapisuje każdy
eksport i log procesu w `runs/`, wszystkie obserwacje w `raw_runs.csv` oraz
końcową decyzję w `manifest.json`.

Najkrótszy czas i panel przechodzą tylko przy p95 błędu bezwzględnego `<= 0.01`,
Spearmanie `>= 0.90` i zgodności top-1 `>= 95%`. Promocja wariantu wymaga na
niezależnym potwierdzeniu:

- dolnej jednostronnej granicy bootstrap różnicy score większej niż `-0.01`;
- średniej różnicy dla każdego celu co najmniej `-0.01`;
- częstości ścisłej wykonalności nie gorszej od legacy o więcej niż 5 pp.;
- redukcji duplikatów o co najmniej 50%;
- zera prób zmieniających wyłącznie martwe pola.

Repozytorium nie zawiera zmyślonych wyników. Dopóki pełne artefakty nie istnieją
i bramki nie przejdą, `legacy-v1` pozostaje domyślną ścieżką.
