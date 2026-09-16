# P-06 — prawdziwe seedy i odporność wyniku na losowość

**Status:** `planned`
**Priorytet:** P0
**Zależność wejściowa:** P-01 (`implemented`)
**Następne zadania:** P-02, następnie P-10
**Źródło statusu:** [`NMM_DEVELOPMENT_REGISTER.md`](NMM_DEVELOPMENT_REGISTER.md)
**Zależność DSP przed implementacją:** [`DSP_SEEDED_ENGINE_API_PLAN.md`](DSP_SEEDED_ENGINE_API_PLAN.md)

Zmiany seedów wewnątrz DSP zostały odebrane jako osobne zadanie. Wydanie DSP
`v0.4.0` i zdalny `master` wskazują commit
`81b51fad005bb6522cbbf42ef08ca4a3c6c9ab06`; pełna walidacja przeszła 7/7,
w tym niezależny replay 1 344 renderów. NMM przypina dokładnie ten commit.
P-06 pozostaje w statusie `planned`, ponieważ implementacja rozdzielenia
seedów wewnątrz NMM nie została jeszcze rozpoczęta.

## 1. Cel

Po wykonaniu P-06 liczba podana jako `--seed` ma naprawdę sterować całym
stochastycznym przebiegiem oceny, a nie być tylko informacją w raporcie.

Chcemy uzyskać cztery własności:

1. Ten sam preset, konfiguracja, seed i wersja programu dają ten sam wynik.
2. Różne realizacje używają różnych, nazwanych strumieni pseudolosowych.
3. Porównywane presety dostają te same scenariusze losowe, aby przypadek nie
   faworyzował jednego z nich.
4. Wynik końcowy pokazuje zmienność i potrafi powiedzieć „brak rozstrzygnięcia”,
   zamiast zawsze nazywać jeden preset najlepszym.

P-06 zapewnia powtarzalność obliczeń i uczciwsze porównanie symulacji. Nie jest
dowodem, że NMM trafnie przewiduje odpowiedź człowieka; to wymaga P-12.

## 2. Co jest dziś niepoprawne lub niepełne

| Miejsce | Stan obecny | Skutek |
|---|---|---|
| `SimulationConfig.reproducibility_seed` | Seed jest opisany jako metadata-only | `--seed` nie steruje oceną |
| `NumericParamsSnapshot` | Zawsze zapisuje `jr_stochastic_rng_seed = 42` | Raport sugeruje jeden wspólny seed JR |
| `JansenRitModel` | Każda instancja zaczyna od xorshift64 ze stanem `42` | Osiem kolumn może dostać identyczny szum |
| `MovementController` | Random walk używa stałej, indeksu i fazy | Główny seed nie steruje ruchem |
| DSP | Szum obiektów, anchor i losowe modulatory mają stałe seedy konstruktora | Główny seed nie steruje audio |
| `auditory::room_impulse` | Losowa odpowiedź pomieszczenia używa stałego xorshift | Realizacja pomieszczenia jest ukryta i nie podlega `--seed` |
| `DifferentialEvolution` | `--seed` steruje `StdRng`, ale ten sam seed jest wpisywany do konfiguracji symulacji | Seed wyszukiwania jest pomieszany z seedem badanego zjawiska |
| `generate-data` | Własny xorshift generuje genomy, a każda ocena dostaje ten sam metadata-only seed | CSV nie opisuje faktycznej realizacji |
| `evaluate` | Brak argumentu `--seed` i liczby realizacji | Oceny nie da się jawnie odtworzyć z CLI |
| CSV | `seed_eval`, `score_std` i `repeats` już istnieją, lecz często są puste lub mają `repeats=1` | Schemat obiecuje więcej niż wykonuje kod |
| `disturb` | Lewe i prawe impulsy korzystają ze stałych seedów | Seed eksperymentu nie obejmuje zakłócenia |

Istniejący xorshift64 nie jest dobrym docelowym RNG do badań symulacyjnych.
Proste generatory xorshift mają znane słabości statystyczne, a stan zerowy jest
stanem zablokowanym. P-06 zastąpi go w ścieżkach NMM jawnym, wersjonowanym RNG.

## 3. Wnioski z researchu

### 3.1. Strumienie trzeba rozdzielać po adresie, nie przez dodawanie liczb

NumPy `SeedSequence` stosuje mieszanie z efektem lawinowym oraz ścieżkę w
drzewie seedów. Dokumentacja ostrzega przed schematem `root_seed + worker_id`,
ponieważ różne uruchomienia mogą wtedy użyć nakładających się seedów. Random123
pokazuje tę samą ogólną zasadę z innej strony: klucz i licznik pozwalają tworzyć
niezależnie adresowane strumienie bez zależności od kolejności wykonania.

Wniosek dla NMM: każdy strumień dostaje trwały adres, np.
`neural/left/band/2`, zamiast „następnej liczby pobranej z globalnego RNG”.

### 3.2. RNG będący szczegółem biblioteki nie jest kontraktem reprodukcji

Dokumentacja Rust `StdRng` mówi wprost, że algorytm jest nieprzenośny i może
zmienić się w przyszłej wersji biblioteki. `rand_chacha` udostępnia natomiast
jawne warianty ChaCha z przenośnym, deterministycznym strumieniem i testami
wektorów referencyjnych.

Wniosek dla NMM: używamy bezpośrednio przypiętego `ChaCha12Rng`, zapisujemy jego
wersję w sygnaturze i dodajemy test znanego wyniku. Nie używamy `StdRng` w
żadnej ścieżce objętej obietnicą reprodukcji.

### 3.3. Kandydatów trzeba porównywać parami na tych samych realizacjach

Metoda common random numbers (CRN) zmniejsza wariancję różnicy pomiędzy
symulowanymi wariantami. Literatura ranking-and-selection łączy CRN z
przedziałem obojętności: dwie alternatywy różniące się mniej niż wcześniej
ustalone `delta` mogą zostać uznane za praktycznie nierozróżnialne.

Wniosek dla NMM: każdy finalista dostaje tę samą listę seedów. Porównujemy
wektory wyników parami, a nie dwa niezależne zestawy średnich.

### 3.4. Jedna realizacja nie opisuje wyniku stochastycznego

Badania symulacyjne powinny podawać liczbę replikacji, sposób rozdzielenia
strumieni, długość przebiegu i niepewność wyniku. Nie istnieje naukowo
uniwersalna liczba seedów dobra dla każdego modelu; domyślną liczbę replikacji
trzeba dobrać w benchmarku precyzji.

Wniosek dla NMM: implementujemy poprawne statystyki i osobny benchmark, który
wybierze domyślną liczbę realizacji. Liczba ta będzie częścią wersjonowanej
polityki, a nie „magiczną stałą” ukrytą w kodzie.

### 3.5. Reprodukcja nie naprawia modelu stochastycznego

Ableidinger, Buckwar i Hinterleitner opisują stochastyczny JR jako układ z
losowym wejściem oraz analizują metody numeryczne zachowujące własności układu.
P-06 rozdzieli ścieżki losowe i umożliwi ich odtworzenie, ale nie będzie
zmieniać miejsca dyfuzji, parametrów szumu ani integrowania SDE. Taka zmiana
wymaga osobnego zadania naukowo-numerycznego i nowych baseline'ów.

## 4. Docelowy kontrakt seedów

### 4.1. Jeden seed uruchomienia, wiele niezależnych domen

Publiczne CLI zachowuje prosty argument:

```text
--seed <u64>
```

Jest to `run_seed`, czyli korzeń całego eksperymentu. Wartość `0` jest
dozwolona. Kod nie przekazuje jej bezpośrednio do xorshift ani nie tworzy
seedów przez dodawanie indeksu.

Z korzenia wyprowadzamy adresowane ścieżki:

```text
run_seed
├── optimizer
├── dataset_genome / sample_slot
├── evaluation / search_panel / replicate
│   ├── audio
│   ├── movement / object_slot
│   ├── neural / hemisphere / band
│   ├── environment / room_impulse
│   └── disturbance / hemisphere
└── evaluation / finalist_panel / replicate
    ├── audio
    ├── movement / object_slot
    ├── neural / hemisphere / band
    ├── environment / room_impulse
    └── disturbance / hemisphere
```

Nie dodajemy identyfikatora presetu do ścieżki realizacji. Dzięki temu preset A
i preset B oceniane dla `replicate=3` dostają ten sam scenariusz losowy. Jest
to CRN. Numer slotu obiektu pozostaje częścią adresu, więc włączenie obiektu 7
nie zmienia strumieni obiektów 0–6.
Identyfikatory celu i profilu także nie wchodzą do adresu: ta sama realizacja
oznacza ten sam bazowy scenariusz przy porównywaniu presetów, celów i profili.

### 4.2. `SeedTreeV1`

Powstaje moduł `src/reproducibility.rs` z typowanymi domenami, bez dowolnych
napisów przekazywanych przez miejsca wywołania.

`SeedTreeV1` używa:

- kontekstu BLAKE3: `com.noisegenerator.nmm.seed-tree.2026-09-05.v1`,
- materiału wejściowego: `run_seed`, liczba elementów ścieżki, a następnie
  kolejne identyfikatory domeny i indeksy; każde pole jest zapisane jako
  `u64 little-endian`,
- pełnych 32 bajtów wyniku jako seeda `ChaCha12Rng`,
- pierwszych 8 bajtów little-endian wyłącznie dla API wymagających `u64`.

Identyfikatory domen są enumem z jawnymi numerami. Raz opublikowanego numeru
nie wolno użyć ponownie ani zmienić. Zmiana algorytmu, kontekstu, kodowania albo
adresów oznacza `SeedTreeV2`, zmianę sygnatury i nowe goldeny.
Ścieżka jest zawsze wyprowadzana od korzenia z pełnego adresu; nie łańcuchujemy
wyników kolejnych wywołań KDF. Typowany builder pilnuje dozwolonej gramatyki
adresu. Dokładna tabela numerów domen trafia do kodu, dokumentacji i known-answer
tests przed podłączeniem pierwszego konsumenta.

Pierwsza wersja zamraża następujące identyfikatory i adresy:

| Element | ID | Parametry adresu |
|---|---:|---|
| `optimizer` | 1 | brak |
| `dataset_genome` | 2 | `sample_slot` |
| `evaluation` | 3 | `panel`, `replicate_index`, `consumer`, indeksy konsumenta |
| panel `direct` | 1 | — |
| panel `search` | 2 | — |
| panel `finalist` | 3 | — |
| panel `dataset` | 4 | — |
| panel `disturb` | 5 | — |
| consumer `audio` | 1 | brak |
| consumer `movement` | 2 | `object_slot` |
| consumer `neural` | 3 | `hemisphere`, `band` |
| consumer `environment_rir` | 4 | brak |
| consumer `disturbance` | 5 | `hemisphere` |

`hemisphere` ma wartość `0=left`, `1=right`; `band` ma stabilny indeks `0..3`;
`object_slot` jest indeksem w oryginalnym presecie, a nie numerem na liście
aktywnych obiektów. Dla adresu `evaluation` liczba elementów obejmuje panel,
replikę, konsumenta i jego indeksy. Nieznany ID, zła liczba parametrów albo
indeks spoza zakresu jest błędem, nie fallbackiem.

Powód użycia BLAKE3 jest praktyczny: jego tryb `derive_key` został zaprojektowany
do wyprowadzania osobnych 32-bajtowych podkluczy dla nazwanych kontekstów. Nie
używamy go tu do bezpieczeństwa haseł, tylko do stabilnego rozdzielenia domen.

### 4.3. RNG poszczególnych warstw

| Warstwa | Docelowy RNG | Kontrakt |
|---|---|---|
| DE | `ChaCha12Rng` z domeny `optimizer` | Ten sam seed daje tę samą trajektorię DE |
| Generowanie genomów | osobny `ChaCha12Rng` na `sample_slot` | Liczba wątków nie zmienia genomu |
| Ruch random walk | `ChaCha12Rng` na `object_slot` | Obiekty nie współdzielą stanu |
| Stochastic JR | `ChaCha12Rng` na `(hemisphere, band)` | Osiem kolumn ma osiem różnych strumieni |
| DSP noise/modulatory | seeded constructor DSP i wersjonowany `dsp_seed_tree_v1` | NMM przekazuje domenę `audio` |
| Odpowiedź impulsowa pomieszczenia NMM | `ChaCha12Rng` z domeny `environment/room_impulse` | Ta sama realizacja pomieszczenia dla porównywanych presetów |
| Disturbance | osobny seed lewy/prawy | Impuls nie współdzieli RNG z JR |

Transformacja normalna JR pozostaje jawnie wersjonowana jako
`box_muller_open01_v1`. Z każdego `u64` bierzemy górne 53 bity jako `x` i
liczymy `u=(x+0.5)/2^53`, co daje otwarty przedział `(0,1)` i wyklucza `ln(0)`.
Jedna próbka normalna zużywa dokładnie dwa takie uniformy i zwraca gałąź z
cosinusem; druga gałąź nie jest buforowana. Zmiana transformacji lub liczby
pobrań wymaga zmiany wersji RNG w sygnaturze.

DSP zachowuje stare konstruktory dla aplikacji produkcyjnej. NMM wyprowadza
`u64` dla domeny `audio` i tworzy silnik przez
`NoiseEngine::seeded(sample_rate, master_gain, audio_seed)` albo
`NoiseEngine::seeded_with_source_count(sample_rate, master_gain, source_count,
audio_seed)`. Wewnętrzne drzewo DSP stosuje analogiczne pełne adresy i osobny kontekst
`com.noisegenerator.dsp.seed-tree.2026-09-05.v1`. Zamrożone domeny DSP to
`anchor_left=1`, `anchor_right=2`, `object_noise=3`, `object_bass_mod=4`,
`object_satellite_mod=5` i `object_nature=6`; domeny obiektowe otrzymują
oryginalny `object_slot`. Dwa liście anchoru wybierają wspólną parę stanów
zgodnie z `dsp_correlation_v1`, dlatego nie deklarujemy niezależności kanałów
anchoru. Seed obejmuje:

- szum anchoru lewego i prawego,
- szum każdego slotu źródła,
- stochastic/random-pulse bass modulator,
- stochastic/random-pulse satellite modulator,
- źródła natury, gdy zostaną udostępnione przez format presetu.

Stałe topologie algorytmów pogłosu mogą pozostać parametrem wersji renderera,
jeżeli nie są traktowane jako losowa realizacja eksperymentu. Decyzja ta musi
być zapisana w inwentarzu RNG; nie może pozostać przypadkowym pominięciem.
Generatory używane wyłącznie do stałych danych testowych i testów walidacyjnych są
oznaczone jako dane testowe i nie podlegają `run_seed`, ale ich wynik blokuje
golden. `rand_chacha 0.3.1`, który już występuje w `Cargo.lock`, staje się
bezpośrednią, dokładnie przypiętą zależnością.

## 5. Kontrakt pojedynczej i wielokrotnej oceny

### 5.1. Pojedyncza realizacja

`SimulationConfig` przestaje zawierać metadata-only `Option<u64>`. Otrzymuje
jawny `SeedPolicy`:

```text
LegacyFixedV1
DomainSeparatedV1 {
    run_seed,
    panel,
    replicate_index
}
```

`LegacyFixedV1` służy wyłącznie do odtwarzania baseline'u P-01 i starych
eksportów. Nowe komendy domyślnie używają `DomainSeparatedV1`.

### 5.2. Wielokrotna realizacja

Powstaje wspólne API `evaluate_preset_replicated`. Zwraca:

- dokładną listę tożsamości realizacji
  `(run_seed, panel, replicate_index, seed_tree_revision)`,
- wynik oraz najważniejsze metryki dla każdej realizacji,
- średnią arytmetyczną,
- odchylenie standardowe próby z mianownikiem `n-1`,
- błąd standardowy `sd / sqrt(n)`,
- dwustronny 95% przedział ufności t-Studenta dla średniej,
- minimum i maksimum,
- `n`.

Dla `n=1` pola `sample_std`, `standard_error` i `confidence_interval_95` są
`null`, a nie `0`. NaN lub nieskończoność kończą ocenę czytelnym błędem.

### 5.3. Porównanie finalistów

P-06 dostarcza mechanizm oceny odporności, z którego później korzysta P-10:

1. Po zwykłym wyszukiwaniu wybieramy pięć różnych, kanonicznych finalistów.
2. Każdy finalista dostaje ten sam, rozłączny od wyszukiwania panel seedów.
3. Wyniki porównujemy przez różnice sparowane dla każdego seeda.
4. Liderem obserwowanym jest preset z najwyższą średnią. Remis porządkuje
   kanoniczny hash presetu, ale sam tie-break nigdy nie daje statusu `unique`.
5. Dla `K` finalistów liczymy wszystkie `m = K*(K-1)/2` par. Dla wektora
   różnic `d` przedział ma postać
   `mean(d) ± t(1-alpha/(2m), n-1) * sample_sd(d)/sqrt(n)` przy
   rodzinnym `alpha=0.05`.
6. Domyślna praktyczna granica różnicy to `delta=0.01` na aktualnej skali
   score. Jest zapisywana w raporcie i można ją zmienić przez CLI.
7. Wynik ma status `unique`, tylko jeżeli dolna granica przedziału lidera
   względem każdego rywala jest większa od `delta`.
8. W przeciwnym razie wynik ma status `inconclusive`. Zbiór nierozstrzygnięty
   zawiera lidera oraz każdego rywala, dla którego dolna granica różnicy
   lider−rywal nie przekracza `delta`. Brak dowodu różnicy nie jest nazywany
   równoważnością.
9. Jeżeli po kanonicznej deduplikacji został mniej niż dwóch kandydatów,
   zwracamy `insufficient_distinct_candidates`, a nie `unique`.

Jest to konserwatywny kontrakt inżynierski oparty na CRN i porównaniach
wielokrotnych. Nie opisujemy go jako klinicznej ani biologicznej istotności.
Przedział t zakłada, że średnia sparowanych różnic jest dostatecznie regularna;
raport pokaże rozkład różnic i nie będzie twierdził formalnej gwarancji
probability-of-correct-selection. `delta=0.01` jest progiem inżynierskim na
obecnej skali i musi zostać ponownie ustalona po P-08.
P-10 zdecyduje, jak wieloseedową ocenę włączyć w sam przebieg optymalizacji.

## 6. CLI i artefakty

### 6.1. CLI

Planowane argumenty:

```text
evaluate --seed 42 --replicates N
optimize --seed 42 --search-replicates N --finalist-count 5 \
         --finalist-replicates N --indifference-delta 0.01
optimize-staged --seed 42 --search-replicates N --finalist-count 5 \
                --finalist-replicates N --indifference-delta 0.01
generate-data --seed 42 --replicates N
disturb --seed 42 --replicates N
```

`--seed` zawsze oznacza korzeń uruchomienia. Seed DE, seed generowania genomów i
seedy ewaluacji są z niego wyprowadzane, ale pozostają rozdzielone domenami.
CLI drukuje `run_seed`, rewizję drzewa oraz liczbę realizacji.

`evaluate` i `disturb` domyślnie używają liczby realizacji wybranej w benchmarku.
`optimize` do czasu P-10 domyślnie używa jednej realizacji w wyszukiwaniu i
benchmarkowej liczby dla finalistów. `generate-data` domyślnie zachowuje jedną
realizację na genom ze względu na koszt, ale jawnie raportuje brak estymacji
niepewności. P-06 nie może być wydane, dopóki manifest nie zawiera wybranych
wartości domyślnych.

### 6.2. Sygnatura i replay

`ModelSignature` przechodzi na schema 3 i zapisuje:

- `seed_derivation_revision`,
- `run_seed`, `panel` i `replicate_index`,
- algorytm RNG DE, ruchu i JR,
- rewizję RNG renderera DSP,
- wersję transformacji normalnej,
- dokładny commit DSP jak w P-01.

Agregat wieloseedowy zapisuje dodatkowo dokładną listę realizacji. Sam korzeń
nie wystarcza jako dowód, jeżeli użytkownik podał niestandardową listę seedów.

`replay-export` odtwarza wszystkie realizacje, sprawdza wyniki jednostkowe i
agregat. Schema 2 jest mapowana na `LegacyFixedV1`; nie wolno cicho odtwarzać
starego eksportu nowym drzewem seedów.

### 6.3. CSV

Istniejące pola zostają naprawdę wypełnione:

- `seed_eval` — 64-bitowy skrót korzenia konkretnej realizacji dla zgodności
  z obecnym CSV; autorytatywną tożsamością są cztery pola opisane wyżej,
- `score_mean` — średnia grupy realizacji,
- `score_std` — odchylenie standardowe próby albo puste dla `n=1`,
- `repeats` — faktyczna liczba realizacji.

Każda realizacja ma osobny wiersz z identyfikatorem grupy. Kolejność wierszy
jest sortowana po stabilnym kluczu, nie po kolejności zakończenia wątków.
Timestamp pozostaje metadanymi operacyjnymi i nie uczestniczy w porównaniu
reprodukcji payloadu naukowego.

## 7. Plan implementacji

### Etap 0 — zamrożenie stanu wejściowego

1. Dodać kompletny inwentarz wszystkich produkcyjnych RNG w NMM i DSP.
2. Zapisać hash audio i wynik obecnej ścieżki `LegacyFixedV1`.
3. Potwierdzić, że manifest P-01 przechodzi bez zmian.

### Etap 1 — wersjonowane drzewo seedów

1. Dodać `src/reproducibility.rs` z `SeedTreeV1`, enumami domen i typami paneli.
2. Dodać BLAKE3 i bezpośrednią, przypiętą zależność
   `rand_chacha = "=0.3.1"`; BLAKE3 przypiąć jako `blake3 = "=1.8.2"`,
   zgodnie z wydanym DSP `v0.4.0` (crate z `links` nie pozwala rozwiązać dwóch
   różnych dokładnych wersji w jednym grafie Cargo).
3. Dodać wektory known-answer dla `run_seed` 0, 1, 42 i `u64::MAX`.
4. Zabronić dowolnych stringów jako domen w kodzie produkcyjnym.

### Etap 2 — neural i movement

1. Konstruktor JR przyjmuje pełny seed konkretnej półkuli i pasma.
2. `simulate_bilateral` otrzymuje przygotowany `NeuralSeedPlan` zamiast jednej
   liczby lub ukrytej stałej.
3. P-06 nie zmienia chwili podania szumu, warm-upu ani postaci SDE; zmienia
   wyłącznie źródło i adres strumienia. Ewentualna korekta numeryczna dostaje
   osobne zadanie i baseline.
4. Random walk otrzymuje seed po numerze slotu i nie używa fazy jako entropii.
5. Odpowiedź impulsowa pomieszczenia dostaje seed domeny `environment`.
6. Usunąć produkcyjne xorshift64 z JR, movement, room impulse i generatora
   datasetu.

### Etap 3 — seeded DSP

1. W repo DSP dodać seedowany konstruktor zachowujący stary konstruktor jako
   jawny legacy.
2. Rozdzielić anchor, obiekty i dwa modulatory na stałe adresy domenowe.
3. Dodać testy hashy audio dla tego samego i różnych seedów.
4. Zachować test renderowania bez alokacji i benchmark czasu callbacku.
5. Podbić `RENDERER_REVISION`, utworzyć commit DSP i przypiąć NMM do jego SHA.

### Etap 4 — pipeline, CLI i sygnatura

1. Wprowadzić `SeedPolicy` i usunąć opis metadata-only.
2. Doprowadzić seed do renderera, movement, JR, room impulse i disturb.
3. Rozdzielić domenę RNG optymalizatora od domen ewaluacji.
4. Dodać argumenty CLI i walidację `replicates >= 1`,
   `finalist_replicates >= 2`, `finalist_count >= 2`.
5. Wprowadzić schema 3 z adapterem schema 2 → `LegacyFixedV1`.

### Etap 5 — agregacja i finaliści

1. Dodać `evaluate_preset_replicated` oraz typ wyniku agregowanego.
2. Zaimplementować statystyki z testami na ręcznie policzonych przykładach.
3. Dodać wspólny panel seedów dla kandydatów i oddzielny panel finalistów.
4. Dodać sparowane porównania, korektę wielokrotną i status
   `unique`/`inconclusive`.
5. Eksportować wyniki jednostkowe, agregat i nierozstrzygnięty zbiór.

### Etap 6 — dataset, równoległość i replay

1. Każdy genom i każda realizacja datasetu dostają adresowany seed.
2. Wynik naukowy dla `threads=1` i `threads=4` jest identyczny po stabilnym
   sortowaniu.
3. Wypełnić istniejące pola seed/statystyki w CSV.
4. Rozszerzyć replay o pełny panel realizacji i wykrywanie zmiany seeda.

### Etap 7 — benchmark liczby realizacji

Powstaje wersjonowany manifest P-06 obejmujący:

- pięć stałych presetów testowych: `p06_noise_static`, `p06_noise_random_pulse`,
  `p06_noise_random_walk`, `p06_noise_room` i `p06_tone_control`; manifest
  przechowuje ich SHA-256,
- profile `normal`, `high_alpha`, `adhd`, `aging`, `anxious`,
- cele `focus`, `deep_work`, `sleep`, `deep_relaxation`, `meditation`,
  `isolation`, `shield`, `flow`, `ignition`,
- referencyjny panel 64 seedów,
- badane rozmiary panelu: 4, 8, 16 i 32; dla każdego `N` manifest zawiera
  16 stałych podpaneli będących podzbiorami panelu 64.

Dla każdego rozmiaru porównujemy wynik każdego podpanelu z panelem 64:

1. 95. percentyl bezwzględnego błędu średniego score, liczony po
   presetach, celach, profilach i podpanelach, wynosi `<= 0.01`.
2. Mediana korelacji rang Spearmana pięciu presetów, liczona po kombinacjach
   cel × profil × podpanel, wynosi `>= 0.95`.
3. Zgodność wyboru top-1 lub przynależność do referencyjnego zbioru
   nierozstrzygniętego zachodzi w co najmniej 95% kombinacji
   cel × profil × podpanel.

Domyślnym `finalist_replicates` zostaje najmniejsze `N`, które spełnia wszystkie
trzy warunki. Jeżeli żadne `N <= 32` ich nie spełni, domyślną wartością jest
64. Raw CSV, podsumowanie JSON, czas wykonania i wybrana wartość są zapisywane
w repo. Zmiana modelu lub P-08 wymaga ponowienia tego benchmarku.

## 8. Definition of Done

P-06 można zmienić na `implemented` wyłącznie wtedy, gdy wszystkie poniższe
warunki są spełnione.

### A. Kontrakt i kompletność

- [ ] Inwentarz obejmuje każdy produkcyjny RNG w NMM i DSP; każde pominięcie ma
      jawną decyzję „stały parametr algorytmu” wraz z uzasadnieniem.
- [ ] `--seed` steruje DSP noise/modulatorami, random walk, stochastic JR i
      disturb oraz losową odpowiedzią pomieszczenia; nie istnieje już komentarz
      metadata-only.
- [ ] Seed DE i seed ocenianego zjawiska należą do różnych domen.
- [ ] W nowych ścieżkach nie ma `StdRng`, `thread_rng`, prostego xorshift64 ani
      seeda tworzonego jako `root + index`.
- [ ] Wszystkie wersje RNG i reguła wyprowadzania są w `ModelSignature` schema 3.

### B. Testy deterministyczne

- [ ] Known-answer tests blokują dokładne wyniki `SeedTreeV1` dla seedów
      `0`, `1`, `42` i `u64::MAX`.
- [ ] Ten sam seed i preset dają bitowo identyczny hash audio na kanonicznym
      macOS ARM64.
- [ ] Ten sam seed daje dokładnie ten sam `SimulationResult` i agregat.
- [ ] Inny numer realizacji zmienia hash audio dla presetu szumowego, tor
      random walk, odpowiedź pomieszczenia i ślad JR przy `sigma > 0`.
- [ ] Inny numer realizacji nie zmienia audio presetu `p06_tone_control`, ale
      zmienia jego ślad JR przy `sigma > 0`.
- [ ] Osiem adresów `(2 półkule × 4 pasma)` jest różnych i nie współdzieli stanu.
- [ ] Dodanie nieaktywnego obiektu ani zmiana liczby wątków nie zmienia
      istniejących strumieni.
- [ ] DE z tym samym `run_seed` odtwarza dokładną trajektorię po przejściu na
      jawny `ChaCha12Rng`.
- [ ] Payload datasetu dla `threads=1` i `threads=4` jest identyczny po
      pominięciu timestampów operacyjnych.

### C. Statystyka i uczciwy wynik

- [ ] Testy agregatora potwierdzają mean, sample SD (`n-1`), SE i 95% CI na
      ręcznie policzonym zestawie.
- [ ] Test znanego wyniku potwierdza kwantyl t, liczbę porównań
      `K*(K-1)/2` i wzór przedziału Bonferroniego.
- [ ] Dla `n=1` niepewność ma wartość `null`, a nie zero.
- [ ] Każdy finalista otrzymuje dokładnie ten sam panel seedów CRN.
- [ ] Panel finalistów jest rozłączny z panelem użytym podczas wyszukiwania.
- [ ] Test syntetyczny rozpoznaje jednoznacznego zwycięzcę.
- [ ] Test syntetyczny z małą różnicą zwraca `inconclusive`, nie fałszywe
      „best”.
- [ ] Jeden kanoniczny kandydat po deduplikacji zwraca
      `insufficient_distinct_candidates`.
- [ ] Raport zawiera `delta`, poziom ufności, korektę wielokrotną, `n`, pełną
      listę seedów, wyniki per-seed i ostrzeżenie o założeniach przedziału t.
- [ ] Benchmark 64-seedowy wybiera i zapisuje domyślne `finalist_replicates`
      zgodnie z trzema warunkami z etapu 7.

### D. Replay i kompatybilność

- [ ] `replay-export` odtwarza każdą realizację oraz agregat bez różnic.
- [ ] Zmiana jednego seeda, rewizji drzewa albo RNG jest wykrywana jako błąd.
- [ ] Schema 2 jest odtwarzana tylko jako `LegacyFixedV1`.
- [ ] Manifest i goldeny P-01 pozostają niezmienione w trybie legacy.
- [ ] Nowa ścieżka ma osobny wersjonowany manifest goldenów P-06.

### E. DSP, wydajność i CI

- [ ] DSP seeded constructor ma test tego samego i różnych seedów.
- [ ] Wszystkie testy DSP przechodzą, w tym stos 2 MiB i realtime allocation.
- [ ] Mediana czasu renderowania seeded DSP na kanonicznym benchmarku nie
      pogarsza się o więcej niż 5% względem legacy; raw wynik jest zapisany.
- [ ] `cargo test --locked --all-targets` przechodzi bez `RUST_MIN_STACK`.
- [ ] Pełny discovery Pythona i weryfikacja historycznego baseline'u przechodzą.
- [ ] GitHub Actions jest zielony, włącznie z jobem macOS ARM64.

### F. Dokumentacja

- [ ] `--help` jasno odróżnia run seed, panel wyszukiwania i panel finalistów.
- [ ] Dokumentacja podaje, co jest gwarantowane bitowo, a co tylko numerycznie.
- [ ] Eksport nie nazywa wyniku biologicznie ani klinicznie zwalidowanym.
- [ ] Rejestr zostaje zaktualizowany dopiero razem z kodem, testami, manifestem
      benchmarku i zielonym CI.

## 9. Ryzyka i decyzje, których nie wolno ukryć

1. **Koszt obliczeń:** wieloseedowa ocena mnoży czas pracy. Dlatego liczba
   realizacji wynika z benchmarku, a nie z intuicji.
2. **Zmiana wyników:** nowy RNG i różne strumienie JR zmienią bieżące liczby.
   Jest to oczekiwana, wersjonowana zmiana semantyki; P-01 pozostaje legacy.
3. **Reprodukcja między platformami:** ChaCha i seed tree są przenośne, ale
   FFT, `libm` i działania zmiennoprzecinkowe mogą dać drobne różnice. Obietnica
   bitowa dotyczy kanonicznego macOS ARM64 i przypiętych zależności.
4. **CRN nie zawsze pomaga:** wspólne liczby losowe zwykle zmniejszają wariancję
   różnicy przy dodatniej korelacji, ale nie jest to bezwarunkowa gwarancja.
   Raport benchmarku musi pokazać empiryczną wariancję różnic sparowanych.
5. **Przedział t:** dla małego `n` lub silnie skośnych różnic jego rzeczywiste
   pokrycie może odbiegać od 95%. P-06 nazywa go przybliżonym przedziałem
   inżynierskim i opiera wybór `N` dodatkowo na benchmarku empirycznym; nie
   deklaruje formalnej gwarancji PCS.
6. **Model JR:** P-06 nie zatwierdza sposobu dyskretyzacji stochastic JR. Ten
   temat powinien otrzymać osobny wpis, jeżeli stochastic JR ma stać się
   komponentem dowodowym, a nie tylko hipotezą candidate.

## 10. Podstawa naukowa i techniczna

- Ableidinger, Buckwar, Hinterleitner (2017), *A Stochastic Version of the
  Jansen and Rit Neural Mass Model: Analysis and Numerics* —
  <https://doi.org/10.1186/s13408-017-0046-4>.
- Salmon, Moraes, Dror, Shaw (2011), *Parallel Random Numbers: As Easy as
  1, 2, 3* — <https://doi.org/10.1145/2063384.2063405>.
- Panneton, L'Ecuyer (2005), *On the Xorshift Random Number Generators* —
  <https://doi.org/10.1145/1113316.1113319>.
- Nelson, Matejcik (1995), *Using Common Random Numbers for Indifference-Zone
  Selection and Multiple Comparisons in Simulation* —
  <https://doi.org/10.1287/mnsc.41.12.1935>.
- Glasserman, Yao (1992), *Some Guidelines and Guarantees for Common Random
  Numbers* — <https://doi.org/10.1287/mnsc.38.6.884>.
- Amaran et al. (2016), *Simulation optimization: a review of algorithms and
  applications* — <https://doi.org/10.1007/s10479-015-2019-x>.
- Monks et al. (2019), *Strengthening the reporting of empirical simulation
  studies: Introducing the STRESS guidelines* —
  <https://doi.org/10.1080/17477778.2018.1442155>.
- Sandve et al. (2013), *Ten Simple Rules for Reproducible Computational
  Research* — <https://doi.org/10.1371/journal.pcbi.1003285>.
- NumPy, *Parallel random number generation* —
  <https://numpy.org/doc/stable/reference/random/parallel.html>.
- Rust `rand`, `StdRng` portability contract —
  <https://docs.rs/rand/latest/rand/rngs/struct.StdRng.html>.
- Rust `rand_chacha`, reproducible generators and reference vectors —
  <https://docs.rs/rand_chacha/0.3.1/rand_chacha/>.
- BLAKE3 specification and `derive_key` contract —
  <https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf>.
