# NMM — kanoniczny rejestr rozwoju

**Stan na:** 2026-09-17
**Przejrzana rewizja bazowa NMM:** `99e58bc67d0b9a141becf524386fddc1b1d018ae`
**Rola dokumentu:** jedno miejsce, które mówi, co naprawdę jest w kodzie i co nadal trzeba zrobić.

Ten rejestr łączy:

- [NMM Scientific Audit and Remediation Plan](NMM_SCIENTIFIC_AUDIT_AND_REMEDIATION_PLAN.md),
- [NMM Refactor Plan](nmm_refactor.md),
- [Neural Model Improvement Roadmap](update_model.md),
- [Paper Findings](paper_findings.md),
- plany Stage 8 i Stage 8d,
- review kodu wykonane 2026-09-03,
- audyt RNG i research P-06 wykonane 2026-09-04–05,
- implementację i pełny odbiór P-06 wykonane 2026-09-17,
- oraz review kolejności prac wykonane 2026-09-06.

Stare dokumenty nadal wyjaśniają historię i szczegóły decyzji. Ten plik jest nadrzędnym źródłem **aktualnego statusu**.

## Jak czytać status

Rejestr celowo używa tylko dwóch statusów:

- `implemented` — dokładnie opisany zakres istnieje w kodzie. Nie oznacza to automatycznie, że rozwiązanie zostało potwierdzone na ludziach ani że nie ma ograniczeń.
- `planned` — zakres nie istnieje, jest niepełny albo obecne rozwiązanie trzeba zastąpić. Zadanie uznajemy za `implemented` dopiero po spełnieniu kryteriów odbioru.

Jeżeli większy temat jest wykonany tylko częściowo, dzielimy go na dwa wpisy: wykonany fundament ma status `implemented`, a brakująca część status `planned`. Dzięki temu słowo „wdrożone” nie ukrywa niedokończonej pracy.

Priorytety:

- **P0** — bez tego wynik może być błędny albo niemożliwy do odtworzenia.
- **P1** — duży wpływ na jakość wyboru presetu i wiarygodność modelu.
- **P2** — ważne rozwinięcie, ale można je robić po ustabilizowaniu podstaw.
- **P3** — praca badawcza na później.

Prosty słownik:

- **nośna** — podstawowy kolor i zakres częstotliwości dźwięku,
- **modulacja** — rytmiczna zmiana głośności dźwięku w czasie,
- **pobudzenie / arousal** — przybliżony poziom czuwania i aktywacji,
- **proxy** — pomiar zastępczy; coś, co może być związane z celem, ale nie jest samym celem,
- **seed** — liczba startowa generatora losowego, dzięki której można powtórzyć wynik,
- **golden** — zapisany oczekiwany wynik testu regresyjnego,
- **holdout** — dane odłożone na koniec, których nie wolno używać podczas budowania modelu,
- **surrogate** — szybki model zastępczy, który tylko przewiduje przybliżony score drogiej symulacji.

## Krótkie podsumowanie

| Obszar | Co już mamy | Najważniejszy brak |
|---|---|---|
| Pipeline | Zielone testy, schema 3, wersjonowane drzewo seedów, replikowana ocena i replay | Bitowa gwarancja jest ograniczona do kanonicznego macOS ARM64 |
| Dźwięk | Renderer binauralny, pomieszczenie, gammatone, modulatory | Brak stałej kalibracji poziomu dźwięku do wejścia neuronalnego |
| Legacy NMM | JR, WC, FHN, półkule, habituacja, szum | Nośna dźwięku nadal wybiera rodzinę rytmu korowego |
| Candidate V2 | Oddzielne cechy nośnej i modulacji, diagnostyka | To jeszcze algebraiczny szkic, nie pełny model dynamiczny |
| Cele | Dziewięć opisanych celów, jawne etykiety dowodów | Wagi i idealne pasma są głównie ręcznymi założeniami |
| Optymalizacja | DE, etapy, ograniczenia, surrogate jako filtr i CRN finalistów | Kategorie są nadal traktowane jak liczby, a kontrakt surrogate wymaga naprawy |
| Walidacja | Schemat danych, podział po uczestnikach, benchmarki i raporty | Brak pełnej walidacji na rzeczywistych danych uczestników |

## Indeks zadań

| ID | Status | Priorytet | Krótki zakres |
|---|---|---:|---|
| I-01 | `implemented` | — | Wspólny pipeline i sygnatura modelu |
| I-02 | `implemented` | — | Słuchowe przygotowanie dźwięku |
| I-03 | `implemented` | — | Rdzeń dynamiczny LegacyV1 |
| I-04 | `implemented` | — | CET, ASSR i PLV |
| I-05 | `implemented` | — | Pobudzenie i bramka wzgórzowa |
| I-06 | `implemented` | — | Habituacja i stochastic JR |
| I-07 | `implemented` | — | Diagnostyka i cechy CandidateV2 |
| I-08 | `implemented` | — | Cele i metryki akustyczne |
| I-09 | `implemented` | — | DE i filtr surrogate |
| I-10 | `implemented` | — | Infrastruktura kalibracji EEG |
| I-11 | `implemented` | — | Kontrakt interpretacji wyniku |
| P-01 | `implemented` | P0 | Zielony baseline i CI |
| P-02 | `planned` | P0 | Kalibracja amplitudy i SPL |
| P-03 | `planned` | P0 | Rozdzielenie nośnej od modulacji |
| P-04 | `planned` | P0 | Dynamiczny CandidateV2 |
| P-05 | `planned` | P1 | ASSR z faktycznie wyrenderowanego audio |
| P-06 | `implemented` | P0 | Seedy i odporność na losowość |
| P-07 | `planned` | P1 | Pobudzenie z niepewnością |
| P-08 | `planned` | P0 | Nowy, wielowymiarowy scoring |
| P-09 | `planned` | P1 | Ciągłe profile osób |
| P-10 | `in_progress` | P0 | Optymalizator zmiennych mieszanych; implementacja gotowa, promocja czeka na benchmark |
| P-11 | `planned` | P1 | Kontrakt i walidacja surrogate |
| P-12 | `planned` | P0/P1 | Prawdziwe dane i promocja komponentów |
| P-13 | `planned` | P1/P3 | Granice celu Sleep i przyszły closed-loop |
| P-14 | `planned` | P2/P3 | Dynamiczne półkule i wiele kolumn |
| P-15 | `planned` | P1 | Decymacja bez aliasingu |
| P-16 | `planned` | P0/P1 | Jeden status i poprawna dokumentacja |

## Powiązanie ze starszymi planami

| Starszy wpis | Stan w tym rejestrze |
|---|---|
| Audit F1: nośna pomieszana z rytmem | Fundament rozdzielenia: I-07; dokończenie: P-03 i P-04 |
| Audit F2: ręczne cele pasm EEG | Semantyka celów: I-08; nowy scoring: P-08 |
| Audit F3: `disturb` poza głównym pipeline’em | Kanoniczna ścieżka: I-01; naprawa regresji: P-01 |
| Audit F4: heurystyczne pobudzenie | Obecny gate: I-05; latentne pobudzenie: P-07 |
| Audit F5: ADHD oparte głównie na stochastic resonance | Obecny profil: I-08; nowe profile i kalibracja: P-09 i P-12 |
| Audit F6: ASSR tylko jako gain | Obecna ścieżka: I-04; pełny model odpowiedzi: P-05 |
| Audit F7: uproszczone sprzężenie półkul | Obecny net-effect: I-03; model dynamiczny: P-14 |
| Audit F8: zbyt silne twierdzenia o śnie | Etykiety zakresu: I-11; dalsza granica produktu: P-13 |
| Audit F9: niespójna dokumentacja | P-16 |
| Refactor Stage 0–2 | I-01, I-07 i P-01 |
| Refactor Stage 3 | I-07 i P-03 |
| Refactor Stage 4 | I-07 i P-04 |
| Refactor Stage 5 | I-05 i P-07 |
| Refactor Stage 6–7 | I-08, P-08 i P-09 |
| Refactor Stage 8 | I-10, I-11 i P-12 |
| Refactor Stage 9 | P-12 i zasady promocji w P-16 |
| `update_model.md` Priority 1–13 | I-02–I-07 oraz P-02, P-05, P-15 |
| `update_model.md` Priority 14 | I-09 i P-11 |
| `update_model.md` Priority 28 | I-08, I-09 i P-10 |
| Stage 8/8d | I-10 i P-12 |

---

# Zadania `implemented`

## I-01. Jedna ścieżka oceny i sygnatura modelu

**Status:** `implemented`
**Kod:** [`src/pipeline.rs`](src/pipeline.rs), [`src/model_signature.rs`](src/model_signature.rs), [`src/export.rs`](src/export.rs), [`src/disturb.rs`](src/disturb.rs)

**Problem:** Wcześniej różne komendy mogły liczyć wynik trochę inaczej. Wtedy nie było jasne, czy różnica pochodzi z presetu, czy z innej ścieżki programu.

**Co zrobiono:** Główna ocena przechodzi przez wspólny pipeline. Wynik może zawierać wersję modelu, profil scoringu, flagi i parametry potrzebne do opisania przebiegu. `disturb` ma dostęp do kanonicznej ścieżki oraz jawnego trybu ablation.

**Pomysł rozwiązania:** Każda komenda ma korzystać z tych samych etapów: renderowanie, analiza słuchowa, model korowy, scoring i eksport. Każdy wynik ma zapisywać kompletną sygnaturę wykonania.

**Dlaczego warto:** Bez wspólnej ścieżki nie da się uczciwie porównywać wyników ani znaleźć regresji.

**Prace naukowe i metodyczne:** [Sandve et al. 2013 — zasady odtwarzalnych analiz obliczeniowych](https://doi.org/10.1371/journal.pcbi.1003285), [Wilson et al. 2014 — dobre praktyki oprogramowania naukowego](https://doi.org/10.1371/journal.pbio.1001745).

**Stan po P-06:** `run_seed` steruje objętymi kontraktem strumieniami przez schema 3; schema 2 pozostaje odtwarzana jako jawny `LegacyFixedV1`.

## I-02. Słuchowe przygotowanie dźwięku

**Status:** `implemented`
**Kod:** [`src/auditory/gammatone.rs`](src/auditory/gammatone.rs), [`src/auditory/crossover.rs`](src/auditory/crossover.rs), [`src/pipeline.rs`](src/pipeline.rs), [`src/preset.rs`](src/preset.rs)

**Problem:** Surowej próbki audio nie można podać wprost do modelu mózgu. Najpierw trzeba oszacować, jak energia dźwięku rozkłada się w pasmach słuchowych i w obu uszach.

**Co zrobiono:** Renderer tworzy sygnały lewego i prawego ucha, uwzględnia ruch i pomieszczenie, a filtrbank gammatone dzieli sygnał na grupy częstotliwości. Potem obwiednie są redukowane do częstotliwości potrzebnej przez NMM.

**Pomysł rozwiązania:** Zachować gammatone jako praktyczny front-end, ale wersjonować jego parametry, filtr decymacji i sposób kalibracji poziomu.

**Dlaczego warto:** Jest to potrzebny most między presetem audio a modelem neuronalnym.

**Prace naukowe:** [Hohmann 2002 — analiza dźwięku filtrem gammatone](https://www.amtoolbox.org/amt-1.6.0/doc/models/hohmann2002.php), [Glasberg i Moore — rozwój modeli głośności dźwięków zmiennych w czasie](https://pmc.ncbi.nlm.nih.gov/articles/PMC4227665/).

**Znane ograniczenie:** Obecna normalizacja usuwa dużą część informacji o bezwzględnym poziomie dźwięku; obsługuje to P-02. Prosta decymacja wymaga poprawy w P-15.

## I-03. Dynamiczny rdzeń LegacyV1

**Status:** `implemented`
**Kod:** [`src/neural/jansen_rit.rs`](src/neural/jansen_rit.rs), [`src/neural/wilson_cowan.rs`](src/neural/wilson_cowan.rs), [`src/neural/fhn.rs`](src/neural/fhn.rs), [`src/brain_type.rs`](src/brain_type.rs)

**Problem:** Potrzebny był model, który nie tylko mierzy widmo dźwięku, ale tworzy dynamiczną, EEG-podobną odpowiedź populacji neuronów.

**Co zrobiono:** LegacyV1 zawiera modele Jansen–Rit, Wilson–Cowan i FitzHugh–Nagumo, osobne półkule oraz opóźniony efekt międzypółkulowy.

**Pomysł rozwiązania:** Zachować ten rdzeń jako zamrożony punkt odniesienia. Nowe mechanizmy rozwijać obok niego, pod nową wersją, zamiast po cichu zmieniać znaczenie starego wyniku.

**Dlaczego warto:** LegacyV1 daje użyteczny model dynamiczny i bazę do testów porównawczych.

**Prace naukowe:** [Jansen i Rit 1995 — model kolumn korowych](https://pubmed.ncbi.nlm.nih.gov/7578475/), [Wilson i Cowan 1972 — dynamika populacji pobudzających i hamujących](https://pubmed.ncbi.nlm.nih.gov/4332108/), [Slater i Isaacson 2020 — międzypółkulowy wpływ w korze słuchowej](https://pubmed.ncbi.nlm.nih.gov/32769158/).

**Znane ograniczenie:** Pasmo nośne wybiera dziś rodzinę i parametry rytmu, a półkule nie są sprzężone wewnątrz równań. Obsługują to P-03, P-04 i P-14.

## I-04. CET, ASSR i miary zgodności fazy

**Status:** `implemented`
**Kod:** [`src/auditory/assr.rs`](src/auditory/assr.rs), [`src/auditory/crossover.rs`](src/auditory/crossover.rs), [`src/neural/performance.rs`](src/neural/performance.rs), [`src/pipeline.rs`](src/pipeline.rs)

**Problem:** Kolor szumu nie opisuje rytmu obwiedni. Preset może mieć wolną modulację, 40 Hz albo kilka modulacji naraz i model powinien to widzieć.

**Co zrobiono:** CET rozdziela wolną i szybką część obwiedni. ASSR ma krzywą transferu, a wynik zawiera miary PLV i envelope PLV.

**Pomysł rozwiązania:** Traktować obwiednię jako oddzielny sygnał czasowy, a amplitudę i stabilność fazy raportować osobno.

**Dlaczego warto:** Dzięki temu modulowany brązowy i modulowany biały szum mogą mieć tę samą częstotliwość modulacji, choć różnią się nośną.

**Prace naukowe:** [Yin et al. 2011 — kodowanie modulacji amplitudy w A1](https://pubmed.ncbi.nlm.nih.gov/21148093/), [Ross et al. 2003 — wpływ nośnej na odpowiedź 40 Hz](https://pubmed.ncbi.nlm.nih.gov/14644459/), [Johnson et al. 2024 — rezonans i czasowa stabilność ASSR](https://www.nature.com/articles/s41598-024-66697-4).

**Znane ograniczenie:** Produkcyjny ASSR nadal korzysta głównie z metadanych modulatorów i jednego gainu. Dokończenie opisuje P-05.

## I-05. Jawny model pobudzenia i bramka wzgórzowa

**Status:** `implemented`
**Kod:** [`src/auditory/thalamic_gate.rs`](src/auditory/thalamic_gate.rs), [`src/auditory/physiological_thalamic_gate.rs`](src/auditory/physiological_thalamic_gate.rs), [`src/pipeline.rs`](src/pipeline.rs)

**Problem:** Ten sam dźwięk może dawać inną odpowiedź zależnie od stanu czuwania. Wcześniej stan był ukryty w stałych parametrach.

**Co zrobiono:** Pipeline ma jawny wybór modelu arousal, wartość stałą oraz heurystyczną i fizjologiczną bramkę wzgórzową. Wynik może podać źródło oszacowania pobudzenia.

**Pomysł rozwiązania:** Zachować obecne bramki jako eksperymentalne warianty, ale nie traktować ich jako pomiaru prawdziwego stanu użytkownika.

**Dlaczego warto:** Jawne założenie można testować i zmieniać. Ukrytego założenia nie da się kontrolować.

**Prace naukowe:** [Destexhe et al. 1996 — dynamika komórek TC/RE i oscylacji wzgórzowych](https://pubmed.ncbi.nlm.nih.gov/8890314/).

**Znane ograniczenie:** Mapowanie jasności, ruchu i pogłosu na pobudzenie jest ręczną hipotezą. Kalibrację opisuje P-07.

## I-06. Habituacja i stochastic JR

**Status:** `implemented`
**Kod:** [`src/neural/jansen_rit.rs`](src/neural/jansen_rit.rs), [`src/pipeline.rs`](src/pipeline.rs)

**Problem:** Całkowicie deterministyczny model reaguje identycznie w każdej chwili i nie słabnie przy długim bodźcu. To zbyt proste w porównaniu z działaniem układu nerwowego.

**Co zrobiono:** Dodano krótkotrwałe osłabianie odpowiedzi oraz losowy składnik w równaniach JR.

**Pomysł rozwiązania:** Pozostawić te mechanizmy, ale dokładnie określić miejsce dodawania szumu, jednostki, osobne strumienie losowe i rozkład parametrów.

**Dlaczego warto:** Model może pokazać różnicę między krótką reakcją a długotrwałym działaniem i może mierzyć zmienność wyniku.

**Prace naukowe:** [Buckwar, Ableidinger i Hinterleitner 2017 — stochastyczny Jansen–Rit](https://doi.org/10.1186/s13408-017-0046-4), [Tsodyks i Markram 1997 — depresja synaptyczna](https://pubmed.ncbi.nlm.nih.gov/9012851/).

**Stan po P-06:** Instancje JR mają rozdzielone, adresowane strumienie dla półkuli i pasma. P-06 nie zatwierdza jednak dyskretyzacji stochastic JR ani parametrów szumu; ewentualna promocja wymaga osobnej walidacji numerycznej i P-12.

## I-07. Diagnostyka okresowa/aperiodyczna i cechy CandidateV2

**Status:** `implemented`
**Kod:** [`src/neural/aperiodic.rs`](src/neural/aperiodic.rs), [`src/auditory/features.rs`](src/auditory/features.rs), [`src/neural/candidate_v2.rs`](src/neural/candidate_v2.rs)

**Problem:** Sama moc w paśmie alfa lub gamma nie mówi, czy istnieje wyraźny rytm. Może to być tylko zmiana szerokiego tła widma. Trzeba też oddzielić kolor dźwięku od rytmu obwiedni.

**Co zrobiono:** Dodano diagnostykę tła aperiodycznego, pików okresowych, cechy ślimakowe, widmo modulacji oraz oddzielny namespace CandidateResearchV2.

**Pomysł rozwiązania:** Najpierw pokazywać nowe cechy jako diagnostykę, a dopiero po testach pozwolić im zmieniać produkcyjny wynik.

**Dlaczego warto:** Możemy zobaczyć, z czego naprawdę wynika score, bez natychmiastowego zerwania zgodności LegacyV1.

**Prace naukowe:** [Donoghue et al. 2020 — rozdzielenie okresowych pików i tła aperiodycznego](https://pmc.ncbi.nlm.nih.gov/articles/PMC8106550/), [Yin et al. 2011](https://pubmed.ncbi.nlm.nih.gov/21148093/).

**Znane ograniczenie:** Każde pasmo obwiedni jest dzielone przez własne odchylenie standardowe. Pomaga to wykryć tempo, ale usuwa znaczną część informacji o sile modulacji. CandidateV2 nie jest jeszcze dynamicznym NMM; obsługują to P-03 i P-04.

## I-08. Cele, etykiety dowodów i metryki akustyczne

**Status:** `implemented`
**Kod:** [`src/scoring.rs`](src/scoring.rs), [`src/acoustic_score.rs`](src/acoustic_score.rs), [`docs/practical_evaluation_contract.md`](docs/practical_evaluation_contract.md)

**Problem:** Nazwy takie jak „focus”, „flow” i „sleep” łatwo pomylić z udowodnionym efektem u człowieka. Jednocześnie cele takie jak izolacja mają ważny, czysto akustyczny składnik.

**Co zrobiono:** Dziewięć celów ma opis produktu, proxy neuronalne, proxy akustyczne i poziom dowodów. Dla Shield i Isolation można łączyć score NMM z metrykami akustycznymi. Dostępny jest również tryb ograniczeń komfortu.

**Pomysł rozwiązania:** Zawsze pokazywać osobno: co zmierzono w dźwięku, co przewidział symulator i czego nie potwierdzono na ludziach.

**Dlaczego warto:** Użytkownik nie powinien odczytać wysokiego score jako diagnozy ani gwarancji działania.

**Prace naukowe:** [Donoghue et al. 2020](https://pmc.ncbi.nlm.nih.gov/articles/PMC8106550/), [Klimesch 1999 — indywidualna częstotliwość alfa](https://pubmed.ncbi.nlm.nih.gov/10209231/), [Nigg et al. 2024 — meta-analiza hałasu i ADHD](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/), [Ngo et al. 2013 — zamknięta pętla podczas snu](https://pubmed.ncbi.nlm.nih.gov/23583623/).

**Znane ograniczenie:** Idealne udziały pasm, firing rate i wagi są ręcznie dobrane. Brakująca metryka ograniczenia może zachować się jak brak naruszenia. Nowy scoring opisuje P-08.

## I-09. Optymalizacja DE i surrogate jako filtr

**Status:** `implemented`
**Kod:** [`src/optimizer/differential_evolution.rs`](src/optimizer/differential_evolution.rs), [`src/surrogate.rs`](src/surrogate.rs), [`tools/train_surrogate.py`](tools/train_surrogate.py), [`src/main.rs`](src/main.rs)

**Problem:** Pełna symulacja każdego kandydata jest kosztowna, a ręczne strojenie setek parametrów jest bardzo trudne.

**Co zrobiono:** Differential Evolution tworzy kandydatów, działa etapami i może używać ograniczeń. MLP przewiduje przybliżony score i służy wyłącznie do wstępnego rankingu. Wybrani kandydaci oraz wynik końcowy są ponownie oceniani prawdziwym pipeline’em.

**Pomysł rozwiązania:** Surrogate ma oszczędzać czas, ale nigdy nie może sam zatwierdzić finalnego presetu.

**Dlaczego warto:** Zachowujemy kontrolę jakości, a jednocześnie możemy przeszukać więcej wariantów.

**Prace naukowe:** [Storn i Price 1997 — Differential Evolution](https://doi.org/10.1023/A:1008202821328), [Jin 2011 — surrogate-assisted evolutionary computation](https://doi.org/10.1016/j.swevo.2011.05.001).

**Znane ograniczenie:** DE źle traktuje kategorie i martwe geny, a kontrakt danych MLP jest niepełny. Obsługują to P-10 i P-11.

## I-10. Infrastruktura kalibracji i publicznych benchmarków EEG

**Status:** `implemented`
**Kod:** [`calibration/README.md`](calibration/README.md), [`tools/calibration`](tools/calibration), [`benchmarks/public_eeg/README.md`](benchmarks/public_eeg/README.md), [`tools/public_eeg_benchmarks`](tools/public_eeg_benchmarks)

**Problem:** Bez stałego formatu danych, podziałów i raportów każdy eksperyment może liczyć metryki inaczej albo przypadkiem użyć tej samej osoby do treningu i testu.

**Co zrobiono:** Są schematy danych, walidator, podział po uczestnikach, holdout, raporty, rejestr publicznych datasetów, kontrola provenance i converter dla ds005048.

**Pomysł rozwiązania:** Utrzymać walidację offline, zapisywać pochodzenie każdego pliku i porównywać rodziny modeli na wspólnych obserwacjach.

**Dlaczego warto:** Jest to podstawa do uczciwego sprawdzenia, czy model przewiduje dane, których wcześniej nie widział.

**Prace naukowe i metodyczne:** [Varoquaux 2018 — duża niepewność cross-validation przy małych próbach](https://pubmed.ncbi.nlm.nih.gov/28655633/), [Pernet et al. 2020 — zalecenia COBIDAS dla EEG/MEG](https://doi.org/10.1038/s41593-020-00709-0), [Gonçalves et al. 2020 — wnioskowanie dla modeli mechanistycznych](https://elifesciences.org/articles/56261).

**Znane ograniczenie:** Obecne artefakty to głównie fixtures i most condition-level. Nie ma pełnej, end-to-end walidacji finalnych celów; opisuje ją P-12.

## I-11. Praktyczny kontrakt interpretacji

**Status:** `implemented`
**Kod:** [`docs/practical_evaluation_contract.md`](docs/practical_evaluation_contract.md), [`stage8_practical_plan.md`](stage8_practical_plan.md), [`src/scoring.rs`](src/scoring.rs)

**Problem:** Wynik symulatora łatwo opisać zbyt mocnym językiem, np. „ten preset poprawia koncentrację”, mimo że model pokazuje jedynie dopasowanie do własnych proxy.

**Co zrobiono:** Cele mają jawne granice interpretacji i poziomy dowodów. Dokumentacja odróżnia cel produktu od twierdzenia biologicznego.

**Pomysł rozwiązania:** Każdy raport powinien mówić „score modelu” lub „proxy”, dopóki osobne badanie nie potwierdzi efektu u ludzi.

**Dlaczego warto:** Chroni to przed błędną interpretacją modelu jako urządzenia diagnostycznego lub klinicznego.

**Prace naukowe:** [Ngo et al. 2013](https://pubmed.ncbi.nlm.nih.gov/23583623/) pokazuje, że dla snu znaczenie ma faza i zamknięta pętla; [Nigg et al. 2024](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/) pokazuje niewielki średni efekt oraz różnice między grupami zamiast uniwersalnej korzyści.

**Znane ograniczenie:** Starsze pliki nadal zawierają mocniejsze lub nieaktualne opisy. Porządki dokumentacyjne opisuje P-16.

## P-01. Zielony i odtwarzalny baseline

**Status:** `implemented`
**Priorytet:** P0
**Kod i artefakty:** [`src/model_signature.rs`](src/model_signature.rs), [`src/export.rs`](src/export.rs), [`baselines/goldens/p01_current_v1.json`](baselines/goldens/p01_current_v1.json), [`.github/CI.md`](.github/CI.md)

**Problem:** Testy regresyjne opisywały stary renderer brown noise, jeden test przepełniał standardowy stos, test discovery Pythona znajdował zero testów, a NMM zależał od przypadkowej lokalnej wersji DSP.

**Co zrobiono:** Zmiana renderera została jawnie nazwana `dsp_brown_hf_v2`, a DSP przypięto do commita `20611c7e2b93e170657cda432f4faca41028f2fe`. Duża pula obiektów DSP jest budowana na stercie i przechodzi test na stosie 2 MiB bez alokacji w ścieżce renderowania. ModelSignature schema 2 zapisuje wersję oraz commit renderera. Komenda `replay-export` odbudowuje konfigurację, ponownie ocenia preset i pokazuje różnice pól. Goldeny są w wersjonowanym manifeście. Historyczny baseline schema 2 pozostaje niezmieniony i może być sprawdzany przez aktualne narzędzie. Publiczne testy Python korzystają z jawnie oznaczonego snapshotu mostu ASSR, a job Rust sprawdza ten snapshot z prawdziwym binarium po pobraniu prywatnego DSP. CI oraz zwykłe `unittest discover` uruchamiają rzeczywiste testy.

**Dlaczego warto:** Kolejne zmiany modelu mają teraz jednoznaczny punkt odniesienia. Różnica wyniku nie jest mylona ze zmianą zależności, platformy ani ukrytym obejściem stosu.

**Prace naukowe i metodyczne:** [Sandve et al. 2013](https://doi.org/10.1371/journal.pcbi.1003285), [Wilson et al. 2014](https://doi.org/10.1371/journal.pbio.1001745).

**Dowód odbioru:** `cargo test --locked --all-targets` — lib 517/517, bridge ASSR 1/1 i główne binarium 679/679, bez `RUST_MIN_STACK`; DSP core 759/759 oraz jawny test stosu 2 MiB; `python3 -m unittest discover -s tests` — 64/64; integralność historycznego baseline — poprawna. Testy pominięte są wyłącznie jawnie oznaczonymi testami eksploracyjnymi/drukującymi.

## P-06. Prawdziwe seedy i odporność na losowość

**Status:** `implemented`
**Priorytet:** P0
**Kod:** [`src/reproducibility.rs`](src/reproducibility.rs), [`src/replicated.rs`](src/replicated.rs), [`src/pipeline.rs`](src/pipeline.rs), [`src/export.rs`](src/export.rs), [`src/bin/p06_replication_benchmark.rs`](src/bin/p06_replication_benchmark.rs)
**Plan i pełny odbiór:** [`P06_SEEDS_AND_ROBUSTNESS_PLAN.md`](P06_SEEDS_AND_ROBUSTNESS_PLAN.md)

**Problem:** Stałe, współdzielone seedy JR i osobne ukryte generatory DSP, ruchu,
pomieszczenia oraz zakłóceń nie podlegały jednemu kontraktowi. Seed DE był
mieszany z seedem symulacji, a wybór najlepszego presetu opierał się na jednej
realizacji.

**Co zrobiono:** `SeedTreeV1` wyprowadza przez BLAKE3 osobne, typowane domeny,
a objęte kontraktem strumienie używają przypiętego `ChaCha12Rng`. Schema 3
zapisuje regułę seedowania i rewizje RNG, natomiast schema 2 jest odtwarzana
wyłącznie jako `LegacyFixedV1`. DSP, ruch, JR, pomieszczenie, zakłócenia,
genomy datasetu i optymalizator otrzymują adresowane strumienie. Replikowana
ocena raportuje wynik per seed, średnią, sample SD, SE oraz 95% CI. Finaliści
korzystają ze wspólnego panelu CRN rozłącznego z wyszukiwaniem i mogą zwrócić
`unique`, `inconclusive` albo `insufficient_distinct_candidates`.

**Benchmark:** Wersjonowany panel obejmuje pięć presetów, pięć profili,
dziewięć celów i 64 realizacje referencyjne. Żaden panel do 32 realizacji nie
spełnił wszystkich progów: dla `N=32` mediana Spearmana i zgodność top-1
przeszły, lecz 95. percentyl błędu średniej wyniósł `0.0139268243` przy limicie
`0.01`. Domyślne `evaluate`, `disturb` i ocena finalistów używają więc 64
realizacji; `generate-data` zachowuje domyślnie jedną realizację.

**Dowód odbioru:** DSP `v0.4.0` jest przypięty do
`81b51fad005bb6522cbbf42ef08ca4a3c6c9ab06`. Implementacja NMM jest w
`68ec6b0bc3e4a3c0e3bab853e8a2d06e38d3a66c` i
`99e58bc67d0b9a141becf524386fddc1b1d018ae`. Pełne testy DSP i NMM,
discovery Pythona, historyczny baseline oraz
[GitHub Actions run 6, attempt 2](https://github.com/lsolniczek/neural_model_for_noise_testing/actions/runs/35168103760)
przeszły. Manifest, surowe 14 400 wierszy i podsumowanie są w
[`benchmarks/p06`](benchmarks/p06/README.md).

**Granica interpretacji:** P-06 zapewnia reprodukcję i uczciwsze porównanie
symulacji. Nie potwierdza biologicznej trafności modelu ani wpływu presetu na
człowieka; ten dowód należy do P-12.

---

# Zadania `planned`

## P-02. Stała kalibracja amplitudy i SPL

**Status:** `planned`
**Priorytet:** P0
**Dotyczy:** Audit F2, `update_model.md` Priority 1, nowe review normalizacji

**Problem:** Pipeline normalizuje każde ucho do własnego maksimum, a FHN dzieli każdy wynik przez jego własny percentyl 95. Przez to cichy i głośny wariant mogą wyglądać dla modelu prawie tak samo.

**Pomysł na rozwiązanie:**

1. Wprowadzić jeden stały poziom odniesienia, np. mapowanie cyfrowego poziomu sygnału na dB SPL dla określonego urządzenia lub profilu kalibracyjnego.
2. Zastąpić normalizację per preset stałym gainem zapisanym w sygnaturze modelu.
3. Zachować ochronę numeryczną wyłącznie jako jawny limiter, nie jako ukrytą normalizację.
4. Rozdzielić głośność, siłę modulacji i kształt widma.
5. Dodać testy: wzrost SPL powinien w określonym zakresie monotonicznie zmieniać drive, a identyczny preset przy tym samym SPL ma dawać ten sam wynik.

**Dlaczego warto:** Poziom dźwięku wpływa na słyszalność, komfort i siłę odpowiedzi. Optymalizator nie powinien traktować `master_gain` jako prawie martwego parametru.

**Prace naukowe:** [Glasberg i Moore — modele głośności dźwięków zmiennych w czasie](https://pmc.ncbi.nlm.nih.gov/articles/PMC4227665/), [Ross et al. 2003](https://pubmed.ncbi.nlm.nih.gov/14644459/).

**Kryteria odbioru:** Brak normalizacji zależnej od maksimum/p95 presetu w candidate path; jawny profil kalibracji; testy monotoniczności; poziom SPL widoczny w eksporcie.

## P-03. Pełne oddzielenie nośnej od modulacji

**Status:** `planned`
**Priorytet:** P0
**Dotyczy:** Audit F1, Refactor Stage 3

**Problem:** LegacyV1 nadal pozwala, aby niskie pasmo nośne wybierało wolniejszy model, a wysokie pasmo szybszy model. Candidate extractor oddziela cechy, ale standaryzuje każde pasmo osobno i przez to osłabia informację o głębokości modulacji.

**Pomysł na rozwiązanie:**

1. Zdefiniować osobne typy danych: `CochlearFeatures`, `TemporalModulationFeatures` i `LevelFeatures`.
2. Nośna ma opisywać miejsce pobudzenia w ślimaku, masking i głośność.
3. Częstotliwość obwiedni ma sterować kandydatem odpowiedzi rytmicznej.
4. Zachować rzeczywistą głębokość i moc modulacji przed standaryzacją.
5. Dodać testy kontrfaktyczne: ta sama modulacja na różnych nośnych powinna zachować dominujące tempo; różna modulacja na tej samej nośnej powinna zmieniać tempo.

**Dlaczego warto:** Usuwa najważniejszą zależność wpisaną z góry do wyniku i pozwala modelowi naprawdę porównywać presety.

**Prace naukowe:** [Yin et al. 2011](https://pubmed.ncbi.nlm.nih.gov/21148093/), [Ross et al. 2003](https://pubmed.ncbi.nlm.nih.gov/14644459/).

**Kryteria odbioru:** Candidate path nie wybiera rytmu na podstawie indeksu pasma nośnego; testy zamiany nośnej i modulacji przechodzą; cechy zachowują tempo i bezwzględną siłę modulacji.

## P-04. CandidateV2 jako rzeczywisty model dynamiczny

**Status:** `planned`
**Priorytet:** P0
**Dotyczy:** Refactor Stage 4, obecny `candidate_v2.rs`

**Problem:** CandidateV2 oblicza odpowiedzi modułów prostymi mnożeniami i ręczną krzywą 40 Hz. Pobiera profil mózgu i arousal, ale nie używa ich do dynamiki. `model_version` nie przełącza głównego modelu korowego.

**Pomysł na rozwiązanie:**

1. Zbudować dynamiczny model odpowiedzi z jawnym wejściem modulacji, poziomu i stanu.
2. Zacząć od małego, identyfikowalnego modelu JR/WC zamiast wielu niekalibrowanych kolumn.
3. Zdefiniować parametry priors jako rozkłady, nie pojedyncze „prawdziwe” wartości.
4. Podłączyć `ModelVersion::CandidateV2` do kanonicznej ścieżki bez zmiany domyślnego LegacyV1.
5. Candidate score liczyć z odpowiedzi candidate, nie z wyjścia legacy.
6. Przed promocją wykonać testy syntetyczne oraz odzyskiwania znanych parametrów.

**Dlaczego warto:** Dopiero wtedy „candidate model” będzie innym NMM, a nie dodatkową formułą scoringową.

**Prace naukowe:** [Jansen i Rit 1995](https://pubmed.ncbi.nlm.nih.gov/7578475/), [Wilson i Cowan 1972](https://pubmed.ncbi.nlm.nih.gov/4332108/), [Gonçalves et al. 2020](https://elifesciences.org/articles/56261).

**Kryteria odbioru:** Candidate ma własny stan w czasie, używa jawnie arousal i profilu parametrów, można go wybrać z CLI/API, eksportuje własny wynik i przechodzi testy syntetyczne bez naruszenia goldenów LegacyV1.

## P-05. ASSR liczony z wyrenderowanej obwiedni

**Status:** `planned`
**Priorytet:** P1
**Dotyczy:** Audit F6, Refactor Stage 2 i 4, `update_model.md` Priority 13

**Problem:** Główna ścieżka ASSR odczytuje kilka typów modulatora z konfiguracji presetu, wybiera jedną częstotliwość i skaluje wszystkie pasma jednym współczynnikiem. Nie widzi pełnego efektu wielu modulatorów po renderowaniu.

**Pomysł na rozwiązanie:**

1. Policzyć widmo modulacji z faktycznej obwiedni w każdym paśmie i uchu.
2. Obsłużyć kilka jednoczesnych częstotliwości zamiast tylko jednej dominującej.
3. Zastosować transfer osobno dla częstotliwości modulacji i grup nośnych.
4. Raportować osobno: amplitudę, vector strength/PLV oraz zmienność opóźnienia między cyklami.
5. Porównywać odpowiedź do kontrolnego, niemodulowanego bodźca.

**Dlaczego warto:** Model będzie oceniał dźwięk, który naprawdę został wyrenderowany, a nie tylko intencję zapisaną w JSON presetu.

**Prace naukowe:** [Yin et al. 2011](https://pubmed.ncbi.nlm.nih.gov/21148093/), [Ross et al. 2003](https://pubmed.ncbi.nlm.nih.gov/14644459/), [Johnson et al. 2024](https://www.nature.com/articles/s41598-024-66697-4).

**Kryteria odbioru:** Każdy typ modulatora może być wykryty z audio; dwa rytmy są widoczne osobno; amplituda i stabilność fazy nie są jednym wynikiem; testy kontrolne bez modulacji nie tworzą sztucznego ASSR.

## P-07. Pobudzenie jako zmienna z niepewnością

**Status:** `planned`
**Priorytet:** P1
**Dotyczy:** Audit F4, Refactor Stage 5

**Problem:** Dzisiejszy gate zmienia jasność, ruch i pogłos w jedną wartość pobudzenia według ręcznie wybranej formuły. Model zachowuje się tak, jakby tę wartość znał dokładnie.

**Pomysł na rozwiązanie:**

1. Traktować arousal jako wejście podane przez użytkownika, rozkład wartości albo osobny model empiryczny.
2. Nie mieszać estymacji arousal z dynamiką wzgórzową w jednej funkcji.
3. Pokazywać score dla kilku możliwych poziomów pobudzenia.
4. Kalibrować mapowanie z użyciem subiektywnej oceny, pupilometrii, tętna/HRV i EEG, jeżeli takie dane zostaną zebrane.
5. Pozostawić obecną heurystykę jako `legacy_heuristic`, nie jako biologiczną prawdę.

**Dlaczego warto:** Ten sam preset może być uspokajający dla jednej osoby i pobudzający dla innej. Model powinien pokazywać tę niepewność.

**Prace naukowe:** [Destexhe et al. 1996](https://pubmed.ncbi.nlm.nih.gov/8890314/), [Nigg et al. 2024](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/).

**Kryteria odbioru:** Źródło arousal jest jawne; można podać rozkład lub serię wartości; raport pokazuje wrażliwość wyniku; brak niekalibrowanego automatycznego gate w candidate default.

## P-08. Scoring oparty na niezależnych pomiarach

**Status:** `planned`
**Priorytet:** P0
**Dotyczy:** Audit F2 i F8, Refactor Stage 6, `update_model.md` Priority 6 i 28

**Problem:** Pięć względnych mocy pasm sumuje się do jedności, więc nie jest pięcioma niezależnymi informacjami. Obecny score porównuje je osobno do ręcznych ideałów, dodaje bonusy PLV i może dojść do limitu 1.0. Brakująca metryka ograniczenia nie zawsze powoduje błąd.

**Pomysł na rozwiązanie:**

1. Rozdzielić wynik na widoczne osie: akustyka, poziom, periodic peaks, aperiodic background, entrainment, stabilność i niepewność.
2. Używać mocy piku ponad tłem zamiast samego udziału pasma.
3. Dostosować granice pasm do indywidualnej częstotliwości alfa, jeśli jest znana.
4. Nie dodawać bonusów poza znormalizowaną sumą wag.
5. Brak wymaganej metryki traktować jako `not_evaluable`, nie jako zero kary.
6. Ręczne cele zachować pod nazwą `legacy_product_heuristic`.
7. Nowe wagi wyznaczać tylko na treningu, a oceniać na zablokowanym holdoucie.

**Dlaczego warto:** Score stanie się łatwiejszy do wyjaśnienia i trudniej będzie go sztucznie podbić kilkoma zależnymi metrykami.

**Prace naukowe:** [Donoghue et al. 2020](https://pmc.ncbi.nlm.nih.gov/articles/PMC8106550/), [Klimesch 1999](https://pubmed.ncbi.nlm.nih.gov/10209231/), [Johnson et al. 2024](https://www.nature.com/articles/s41598-024-66697-4).

**Kryteria odbioru:** Składowe score sumują się według jawnych wag; brak danych nie przechodzi jako brak naruszenia; periodic i aperiodic są osobne; każdy cel ma kartę danych, zakres i źródło wag.

## P-09. Profile osób zamiast etykiet „typ mózgu”

**Status:** `planned`
**Priorytet:** P1
**Dotyczy:** Audit F5, Refactor Stage 7

**Problem:** `Normal`, `HighAlpha`, `ADHD`, `Aging` i `Anxious` są zestawami ręcznych parametrów. Nazwa grupy sugeruje większą pewność, niż daje kod. Osoby w tej samej grupie bardzo się różnią.

**Pomysł na rozwiązanie:**

1. Zastąpić twarde etykiety profilem ciągłych cech: IAF, wiek, słuch, bazowe pobudzenie, reaktywność ASSR, habituacja i parametry E/I.
2. Każdy parametr opisać rozkładem i źródłem danych.
3. Etykiety grupowe zachować jedynie jako opcjonalne priors populacyjne.
4. Nie wyprowadzać jednego mechanizmu ADHD wyłącznie ze stochastic resonance.
5. Wynik raportować jako rozkład po możliwych profilach, nie jedną liczbę.

**Dlaczego warto:** Model będzie personalizowalny i przestanie traktować diagnozę jako jeden sztywny „rodzaj mózgu”.

**Prace naukowe:** [Klimesch 1999](https://pubmed.ncbi.nlm.nih.gov/10209231/), [Nigg et al. 2024](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/), [Donoghue et al. 2020](https://pmc.ncbi.nlm.nih.gov/articles/PMC8106550/).

**Kryteria odbioru:** Profil ma ciągłe parametry i niepewność; etykieta kliniczna nie wybiera bezpośrednio jednego kompletu stałych; eksport pokazuje wykorzystany prior; walidacja jest dzielona po uczestnikach.

## P-10. Optymalizator dla zmiennych mieszanych i realnego presetu

**Status:** `in_progress`
**Priorytet:** P0
**Dotyczy:** `update_model.md` Priority 14 i 28, review przestrzeni optymalizacji

**Problem:** Historyczne dokumenty opisywały genom 230D, ale zamrożona ścieżka
`legacy-v1` ma obecnie 211 pól i nadal odejmuje kody kategorii tak, jakby
`color=2` leżał pomiędzy `color=1` i `color=3`. Nieaktywne obiekty tworzą
martwe wymiary. `spatial_mode` nie był stosowany przez
`Preset::apply_to_engine`, a populacja mniejsza niż cztery mogła zawiesić
`pick_three`. Krótkie okno po warm-upie nie dawało wiarygodnej podstawy do
wyboru czasu oceny i liczby realizacji.

**Pomysł na rozwiązanie:**

1. Natychmiast zastosować `spatial_mode` albo usunąć go z genomu.
2. Walidować populację `>= 4`, zakres `F`, `CR`, `K`, liczbę generacji i czas oceny.
3. Usunąć `source_count` jako niezależny gen, jeśli można go wyliczyć z aktywnych źródeł.
4. Osobno optymalizować strukturę/kategorie i parametry ciągłe.
5. Geny nieaktywnego obiektu wykluczyć z dystansu crowding i mutacji.
6. Użyć odbicia lub resamplingu zamiast samego clampowania granic.
7. Restart i wybór najlepszego mają korzystać z pełnego porównania ograniczeń, nie tylko surowego fitnessu.
8. Stosować adaptacyjne `F`, `CR` i redukcję populacji dopiero po testach porównawczych z obecną metodą.
9. Końcowe etapy liczyć na dłuższych oknach, adekwatnych do najniższej analizowanej częstotliwości.

**Dlaczego warto:** Optymalizator będzie szukał po parametrach, które rzeczywiście zmieniają audio, i przestanie nadawać sztuczny porządek kategoriom.

**Stan implementacji 2026-09-17:**

1. Dodano wersjonowany `mixed-v2` o 423 polach i ośmiu stabilnych slotach.
   Kategorie są dziedziczone lub losowane jako etykiety. Parametry ciągłe są
   mutowane arytmetycznie tylko wtedy, gdy dawcy należą do tej samej aktywnej
   gałęzi. Pola nieaktywne mają wartość kanoniczną i nie uczestniczą w
   dystansie crowding.
2. `source_count` jest wyliczany z masek aktywności, `spatial_mode` jest
   ustalony na ścieżkę obiektową, a nieużywany anchor wyciszony. Genom obejmuje
   między innymi `spread`, Blue i pełny zakres RandomPulse. Kontekst pokoju i
   układu współrzędnych jest zamrożony poza genomem.
3. Nowe ewaluacje używają rewizji `dsp_only_v2`: akustyka pokoju pochodzi tylko
   z DSP. Stare sygnatury bez pola rewizji odtwarzają historyczną ścieżkę z
   dodatkowym syntetycznym RIR.
4. Walidacja odrzuca populację mniejszą niż cztery, błędne `F`, `CR`, liczby
   generacji, panele finalistów i czas nieprzekraczający warm-upu. Dostępne są
   polityki granic clamp, reflect i resample oraz poprawne porównanie ograniczeń
   przy restartach.
5. Jako warianty eksperymentalne dodano crowding, częściowy restart, SHADE z
   pamięcią udanych `F`/`CR` i liniową redukcję populacji. Restart zawsze
   ocenia nowe osobniki przed użyciem ich jako dawców.
6. Eksport zapisuje schemat, rewizję pokoju, seedy, konfigurację DE, hash
   zamrożonego kontekstu, liczniki duplikatów i martwych prób oraz — w trybie
   constrained — wynik ograniczeń. `mixed-v2` jawnie odrzuca stary surrogate i stary format logu do
   czasu wykonania P-11.
7. Dodano dwa odtwarzalne narzędzia: benchmark czasu i liczby realizacji oraz
   dwuetapowe porównanie wariantów optymalizatora. Dokładna procedura znajduje
   się w [`benchmarks/p10/README.md`](benchmarks/p10/README.md).

`legacy-v1` pozostaje domyślny. Zmiana domyślnej ścieżki nastąpi wyłącznie po
pełnym benchmarku rozwojowym i niezależnym potwierdzeniu. Brak artefaktów
benchmarku oznacza brak decyzji o promocji, a nie niepowodzenie implementacji.

**Prace naukowe:** [Storn i Price 1997](https://doi.org/10.1023/A:1008202821328),
[JADE — Zhang i Sanderson 2009](https://doi.org/10.1109/TEVC.2009.2014613),
[L-SHADE — Tanabe i Fukunaga 2014](https://doi.org/10.1109/CEC.2014.6900380),
[Thomsen 2004 — crowding DE](https://doi.org/10.1109/CEC.2004.1331058),
[Takahama i Sakai 2009 — ograniczenia ε](https://doi.org/10.1527/tjsai.24.34),
[Lin et al. 2018 — DE dla zmiennych mieszanych](https://doi.org/10.1016/j.ins.2018.07.035),
[Talbi 2024 — taksonomia optymalizacji mieszanej](https://doi.org/10.1016/j.swevo.2024.101642),
[Glasserman i Yao 1992 — wspólne liczby losowe](https://doi.org/10.1287/mnsc.38.6.884),
[Derrac et al. 2011 — porównania statystyczne](https://doi.org/10.1016/j.swevo.2011.02.002).

**Kryteria odbioru:** Testy kontraktu potwierdzają kanoniczne pola nieaktywne,
brak prób zmieniających wyłącznie martwe pola, poprawne zakresy wszystkich
gałęzi oraz czytelne błędy konfiguracji. Benchmark czasu musi osiągnąć
p95 błędu bezwzględnego `<= 0.01`, Spearmana `>= 0.90` i zgodność top-1
`>= 95%` względem 60 s/64 realizacji. Kandydat może zastąpić `legacy-v1`
tylko wtedy, gdy niezależne potwierdzenie wykaże brak istotnej utraty jakości
dla każdego celu, dolną jednostronną granicę bootstrap powyżej `-0.01`, spadek
duplikatów co najmniej o 50%, zero martwych prób i częstość ścisłej
wykonalności nie gorszą o więcej niż 5 punktów procentowych.

## P-11. Nowy kontrakt i walidacja surrogate

**Status:** `planned`
**Priorytet:** P1
**Dotyczy:** `update_model.md` Priority 14, `paper_findings.md` B2–B4, nowe review datasetu

**Problem:** MLP nie dostaje wszystkich parametrów, które wpływają na etykietę. Trening i runtime inaczej skalują dane. Ten sam genom trafia do losowego train i validation, a zapisywany genom może różnić się od zaokrąglonego presetu, który naprawdę oceniono.

**Pomysł na rozwiązanie:**

1. Utworzyć wersjonowany `SurrogateFeatureSchema` obejmujący pełną sygnaturę wpływającą na score.
2. Zapisywać dokładnie kanoniczny, zaokrąglony genom użyty w symulacji.
3. Zapisywać scalery, hash datasetu, wersję pipeline’u i modelu w pliku wag.
4. Dzielić dane po unikalnym genomie i całym przebiegu optymalizacji.
5. Zostawić oddzielny, nigdy niewidziany zbiór trajektorii.
6. Mierzyć Spearman, top-K recall i regret, a nie tylko MSE/R².
7. Dodać ensemble lub inną miarę niepewności; kandydat o dużej niepewności musi trafić do prawdziwej symulacji.

**Dlaczego warto:** Surrogate ma dobrze wybierać najlepszych kandydatów poza danymi treningowymi, a nie tylko odtwarzać średni score znanych wierszy.

**Prace naukowe:** [Jin 2011](https://doi.org/10.1016/j.swevo.2011.05.001), [Varoquaux 2018](https://pubmed.ncbi.nlm.nih.gov/28655633/), [Gonçalves et al. 2020](https://elifesciences.org/articles/56261).

**Kryteria odbioru:** Brak wycieku genomów między splitami; runtime korzysta ze scalerów z artefaktu; plik wag odrzuca niezgodną sygnaturę; raport zawiera top-K recall/regret i wynik na całych niewidzianych trajektoriach.

## P-12. Walidacja na prawdziwych danych i promocja komponentów

**Status:** `planned`
**Priorytet:** P0 dla twierdzeń o działaniu na ludzi, P1 dla narzędzia badawczego
**Dotyczy:** Audit Stage 5–6, Refactor Stage 8–9, Stage 8c/8d

**Problem:** Repozytorium ma dobry szkielet walidacji, ale obecne fixtures sprawdzają głównie przepływ danych. Most ds005048 przewiduje condition-level siłę gamma/ASSR, a nie pełny wynik presetu. `run_calibration.py` redukuje wiele różnych cech do jednej średniej i dopasowuje prostą regresję.

**Pomysł na rozwiązanie:**

1. Dokończyć źródłowo zweryfikowany przebieg na pełnych plikach ds005048.
2. Najpierw walidować komponenty: wykrycie częstotliwości modulacji, siłę i stabilność ASSR, periodic/aperiodic oraz akustykę.
3. Nie używać walidacji komponentu jako dowodu skuteczności celu.
4. Dla własnego badania wielokrotnie mierzyć te same osoby, dodać warunek kontrolny, losową kolejność i pomiar SPL.
5. Dopasowywać wielowymiarowy model regularizowany lub model efektów mieszanych, zamiast średniej cech i jednej regresji.
6. Utrzymać podział po uczestnikach, nested CV oraz zamknięty holdout.
7. Promować tylko komponent, który spełni wcześniej zapisane progi na holdoucie.

**Dlaczego warto:** To jedyna droga, aby przejść od „symulator lubi ten preset” do „model przewiduje rzeczywistą odpowiedź”.

**Prace naukowe i metodyczne:** [Pernet et al. 2020 — COBIDAS EEG/MEG](https://doi.org/10.1038/s41593-020-00709-0), [Varoquaux 2018](https://pubmed.ncbi.nlm.nih.gov/28655633/), [Yu et al. 2022 — mixed-effects w neuronauce](https://pmc.ncbi.nlm.nih.gov/articles/PMC8763600/), [Gonçalves et al. 2020](https://elifesciences.org/articles/56261).

**Kryteria odbioru:** Co najmniej jeden benchmark korzysta z pełnych, zweryfikowanych danych źródłowych; model ma wynik na zamkniętym holdoucie z przedziałem ufności; raport wyraźnie oddziela walidację komponentu od walidacji celu.

## P-13. Osobny zakres dla snu i ewentualnej pętli zamkniętej

**Status:** `planned`
**Priorytet:** P1 dla dokumentacji, P3 dla systemu closed-loop
**Dotyczy:** Audit F8, Refactor Stage 6, `paper_findings.md` C3

**Problem:** Obecny preset jest open-loop: nie zna fazy snu użytkownika. Nie może więc przewidzieć wzmocnienia wolnych oscylacji, sprzężenia z wrzecionami ani poprawy pamięci w sposób odpowiadający badaniom closed-loop.

**Pomysł na rozwiązanie:**

1. Dla obecnego produktu ograniczyć cel Sleep do maskowania, komfortu i warunków zasypiania.
2. Usunąć z raportów sugestie poprawy pamięci lub slow-wave enhancement.
3. Jeśli powstanie produkt z EEG, utworzyć osobny model i cel `sleep_closed_loop`, z detekcją stadium i fazy w czasie rzeczywistym.
4. Walidować go osobnym protokołem snu, nie danymi z uwagi w dzień.

**Dlaczego warto:** Open-loop i closed-loop są innymi interwencjami. Ich rozdzielenie zapobiega obietnicom, których obecny system nie może sprawdzić.

**Prace naukowe:** [Ngo et al. 2013](https://pubmed.ncbi.nlm.nih.gov/23583623/), [Ngo et al. 2015 — samoograniczający się efekt powtarzanej stymulacji](https://pubmed.ncbi.nlm.nih.gov/25926443/), [Weigenand et al. 2016 — brak korzyści dla pamięci przy open-loop](https://pubmed.ncbi.nlm.nih.gov/27422437/).

**Kryteria odbioru:** Open-loop Sleep ma tylko wspierane proxy; raport nie twierdzi, że wzmacnia wolne fale lub pamięć; closed-loop, jeśli powstanie, ma osobny kontrakt, dane i wersję modelu.

## P-14. Dynamiczne sprzężenie półkul i sieć wielu kolumn

**Status:** `planned`
**Priorytet:** P2/P3
**Dotyczy:** Audit F7, `update_model.md` Priority 10–11, `paper_findings.md` A1 i C1

**Problem:** Dziś półkule są symulowane osobno, a opóźniony sygnał jest odejmowany po symulacji. To może przybliżać końcowy efekt hamujący, ale nie zmienia dynamiki drugiej półkuli w czasie.

**Pomysł na rozwiązanie:**

1. Najpierw poprawić opis obecnego mechanizmu na „zredukowany efekt netto”.
2. Dopiero po kalibracji podstaw wprowadzić sprzężenie wewnątrz równań.
3. Modelować projekcję spoidłową jako pobudzającą, która może rekrutować lokalne interneurony hamujące.
4. Porównać stałe sprzężenie, homeostatyczną kontrolę hamowania i model bez sprzężenia.
5. Multi-column oraz gap junctions traktować jako eksperyment, nie automatyczne ulepszenie.

**Dlaczego warto:** Pozwoli badać opóźnienie, stabilność i synchronizację w sposób przyczynowy, ale dopiero gdy prostsze elementy będą skalibrowane.

**Prace naukowe:** [Slater i Isaacson 2020](https://pubmed.ncbi.nlm.nih.gov/32769158/), [Stasinski et al. 2024 — homeodynamic feedback inhibition](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1012595), [Byrne et al. 2024 — NMM z synapsami elektrycznymi](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1012647).

**Kryteria odbioru:** Sprzężenie działa podczas integracji równań; znak i ścieżka mechanizmu są opisane poprawnie; stabilność jest przetestowana; złożony model musi pobić prosty model na holdoucie.

## P-15. Poprawna decymacja bez aliasingu

**Status:** `planned`
**Priorytet:** P1
**Dotyczy:** `update_model.md` Priority 3

**Problem:** Redukcja 48 kHz do 1 kHz używa prostego uśredniania bloków. Tłumi część wysokich składowych, ale nie daje precyzyjnie zdefiniowanego filtra antyaliasingowego. Energia spoza nowego pasma może trafić w złe miejsce.

**Pomysł na rozwiązanie:**

1. Zdefiniować wymagane pasmo modulacji i pasmo przejściowe.
2. Zastosować wersjonowany filtr FIR/IIR przed decymacją.
3. Zmierzyć charakterystykę amplitudową, opóźnienie i tłumienie aliasów.
4. Dodać testy sinusów poniżej i powyżej częstotliwości Nyquista oraz test zachowania fazy dla PLV.

**Dlaczego warto:** Błąd decymacji może stworzyć nieistniejącą wolną modulację albo zmienić jej fazę, co bezpośrednio wpływa na CET i PLV.

**Prace naukowe i techniczne:** [Hohmann 2002 — kontrolowana analiza filtrowa](https://www.amtoolbox.org/amt-1.6.0/doc/models/hohmann2002.php); standardowa teoria wieloczęstotliwościowego DSP jest tu podstawą inżynierską, nie hipotezą neurobiologiczną.

**Kryteria odbioru:** Jawna specyfikacja filtra; testy tłumienia aliasów i odpowiedzi fazowej; parametry filtra w sygnaturze modelu; porównanie wpływu na istniejące goldeny.

## P-16. Jeden status, poprawne komentarze i polityka twierdzeń

**Status:** `planned`
**Priorytet:** P0 dla statusu/testów, P1 dla opisów naukowych
**Dotyczy:** Audit F9 oraz wszystkie starsze roadmapy

**Problem:** `update_model.md` miesza plan, historię i bieżący status. Twierdzi m.in., że normalizacja p95 zachowuje amplitudę i że szum JR jest podawany w innym miejscu niż robi to obecny kod. Liczby przechodzących testów są nieaktualne. Podczas tworzenia rejestru trzeba było też poprawić błędne „Lu et al. 2011” na Yin et al. 2011.

**Pomysł na rozwiązanie:**

1. Ten rejestr utrzymywać jako jedyne źródło statusu.
2. `update_model.md` zamrozić jako historię lub rozdzielić na `CHANGELOG.md` i dokumenty projektowe.
3. Każdy wpis `implemented` połączyć z kodem, testem i — jeśli dotyczy — artefaktem dowodowym.
4. Nie używać `implemented` jako synonimu „zwalidowane naukowo”.
5. Dodać automatyczny check linków i listy testów w CI.
6. Razem ze zmianą kodu aktualizować komentarze o miejscu szumu, seedach, normalizacji i zakresie celu.
7. Promocję z candidate do default zapisywać jako osobną decyzję z kryteriami oraz wynikiem holdoutu.

**Dlaczego warto:** Zespół i przyszły reviewer zobaczą prawdziwy stan bez czytania kilku sprzecznych roadmap.

**Prace naukowe i metodyczne:** [Sandve et al. 2013](https://doi.org/10.1371/journal.pcbi.1003285), [Wilson et al. 2014](https://doi.org/10.1371/journal.pbio.1001745), [Pernet et al. 2020](https://doi.org/10.1038/s41593-020-00709-0).

**Kryteria odbioru:** Każdy aktywny plan wskazuje ten rejestr; brak sprzecznych statusów; komentarze odpowiadają kodowi; bieżący wynik testów jest generowany automatycznie; promocja modelu wymaga zapisanego dowodu.

---

# Kolejność realizacji

**Aktualny priorytet:** Faza 0 jest zamknięta. Kod kandydata P-10 i procedury
testowe są gotowe; następny krok to wykonanie pełnego benchmarku P-10 i decyzja
o promocji. Potem następują P-11 i infrastruktura P-12 zgodnie z Fazą A.

Dalsza kolejność rozdziela pilne naprawy, przygotowanie walidacji i rozwój modelu.
Zadania P-10, P-11, P-12 i P-13 obejmują kilka etapów, dlatego pojawiają się
w więcej niż jednej fazie z jawnym zakresem. Wykonanie części zadania nie
zmienia jego całego statusu na `implemented`.

P-12 zaczyna się od protokołu i danych rozwojowych. Wyniki na tych danych
kierują rozbudową modelu; zamknięty holdout służy dopiero do końcowej oceny
zamrożonego wariantu. P-16 jest utrzymywane na bieżąco we wszystkich fazach.

## Faza 0 — seedy w DSP i NMM (`completed`)

Punktem odniesienia był P-01 — zielony baseline (`implemented`). Etap został
zamknięty 2026-09-17:

1. DSP wydał seedowane API jako `v0.4.0` na commicie
   `81b51fad005bb6522cbbf42ef08ca4a3c6c9ab06`; NMM przypina pełny SHA.
2. Presety Tone zostały przeniesione na globalne DSP `binaural_beat`, a stare
   presety są migrowane podczas wczytywania.
3. NMM wdrożył schema 3, `SeedTreeV1`, adresowane strumienie wszystkich warstw,
   replikowaną ocenę, CRN finalistów, dataset i replay agregatów.
4. Zamrożony benchmark 64-seedowy wybrał domyślne 64 realizacje i zapisał
   manifest, raw CSV oraz podsumowanie.
5. Pełne testy DSP i NMM, Python, historyczny baseline oraz GitHub Actions na
   macOS ARM64 przeszły.

**Dowody:** commity NMM `68ec6b0` i `99e58bc`, plan odbioru
[`P06_SEEDS_AND_ROBUSTNESS_PLAN.md`](P06_SEEDS_AND_ROBUSTNESS_PLAN.md) oraz
[CI run 6, attempt 2](https://github.com/lsolniczek/neural_model_for_noise_testing/actions/runs/35168103760).

Późniejsze zmiany scoringu, skali wejścia, czasu analizy, modelu
stochastycznego, drzewa seedów, RNG, fixture'ów lub DSP wymagają ponowienia
odpowiednich testów i benchmarku P-06.

## Faza A — pilne naprawy i przygotowanie sprawdzianu

1. P-10, zakres naprawczy (`implemented`) — walidacja populacji `>= 4`,
   parametrów DE i czasu oceny; usunięcie `spatial_mode` z przestrzeni
   wyszukiwania i jawne użycie ścieżki obiektowej.
2. P-11, zakres naprawczy — zgodne skalowanie trening/runtime i jawne
   odrzucanie niezgodnych artefaktów przed dalszym używaniem filtra surrogate.
3. P-12, zakres infrastruktury — naprawa agregacji foldów i przypisywania CI
   w raportach kalibracji; sprawdzenie poprawności podziałów po uczestnikach.
4. P-12, zakres protokołu — wybór danych, mierzonych wyników, prostych modeli
   odniesienia i progów odbioru; wydzielenie danych rozwojowych i zamkniętego
   holdoutu przed strojeniem nowego modelu.
5. P-16 oraz dokumentacyjna część P-13 — aktualny status, dowody odbioru
   i granice interpretacji, w tym zakres open-loop Sleep.

**Warunek przejścia:** podstawowe narzędzia nie zawieszają się na błędnych
wejściach, raporty mają poprawną semantykę, a sprawdzian przyszłego modelu
jest określony przed jego budową.

## Faza B — kalibracja poziomu i stabilność numeryczna

1. P-02 — stała amplituda i jawny profil kalibracji SPL.
2. P-10, zakres czasu oceny (`in_progress`), oraz weryfikacja numeryczna
   rdzenia przed P-04 — narzędzie i progi są zamrożone; pozostaje wykonanie
   benchmarku względem 60 s/64 realizacji i zapis artefaktów.

Korzystamy z seedowania odebranego w fazie 0.
Po zmianie skali wejścia albo czasu analizy trzeba
ponowić benchmark liczby realizacji P-06 dla docelowej konfiguracji.

**Warunek przejścia:** porównanie presetów ma jawną skalę wejścia, realizacje
losowe i okno analizy; wpływ ustawień numerycznych jest zmierzony względem
wcześniej zapisanych tolerancji.

## Faza C — minimalny model i walidacja komponentów

1. P-03 — rozdzielenie nośnej i modulacji.
2. P-15 — kontrolowana decymacja i zachowanie fazy przed odbiorem metryk
   modulacji oraz PLV.
3. P-05 — ASSR z wyrenderowanego audio, z kontrolą niemodulowaną.
4. P-04 — mały dynamiczny CandidateV2 z własnym wynikiem, testami
   syntetycznymi i odzyskiwania parametrów.
5. P-12, zakres komponentów — ocena na danych rozwojowych i porównanie
   z prostymi modelami akustycznymi oraz modulacyjnymi według protokołu fazy A.

**Warunek przejścia:** minimalny candidate przechodzi testy techniczne,
a wyniki walidacji komponentów określają, które mechanizmy warto rozwijać.
Brak przewagi nad prostszym modelem wymaga rewizji hipotezy, zanim dodamy
kolejne parametry i moduły.

## Faza D — scoring i profile uzasadnione danymi

1. P-07 — arousal z niepewnością, rozwijane na podstawie wyników fazy C.
2. P-08 — wielowymiarowy scoring, jawna obsługa brakujących metryk i wagi
   dopasowywane wyłącznie na danych treningowych.
3. P-09 — ciągłe profile osób, w zakresie parametrów możliwych do oszacowania
   z dostępnych danych.
4. P-12, zakres końcowego odbioru — zamrożenie modelu, scoringu i profili;
   ocena na zamkniętym holdoucie oraz decyzja o promocji dla konkretnego
   komponentu lub celu.

**Warunek przejścia:** promocja zależy od wcześniej ustalonych kryteriów
i porównania z baseline'em, a nie od samego uzyskania wyniku z CI.
Holdout nie służy do kolejnych poprawek modelu; strojenie na podstawie jego
wyniku wymaga nowego, niezależnego sprawdzianu przed kolejną promocją.

## Faza E — wydajność wyszukiwania i warunkowa rozbudowa

1. P-10, pełna przebudowa (`in_progress`) — implementacja zmiennych mieszanych,
   masek, crowding, restartów, SHADE i redukcji populacji jest gotowa;
   pozostaje porównanie jakości przy tym samym budżecie prawdziwych ewaluacji
   oraz osobna decyzja o zmianie wartości domyślnej.
2. P-11, pełny zakres — nowy dataset i surrogate dla ustabilizowanego
   kontraktu; ocena top-K recall/regret oraz niewidzianych trajektorii.
3. P-14 — dynamiczne półkule i ewentualna sieć wielu kolumn wyłącznie jako
   eksperyment wymagający wykazania przewagi nad prostszym modelem.
4. P-13, zakres closed-loop — osobny przyszły projekt z własnym protokołem
   snu; nie jest zależnością obecnego narzędzia do projektowania presetów.

Przyspieszenie wyszukiwania nie zastępuje walidacji modelu. Zmiany mechanizmów
w fazie E ponownie przechodzą właściwe sprawdziany P-12 przed promocją.

# Zasada wyboru „najlepszego presetu” do czasu wykonania planu

Do czasu ukończenia P-02, P-03, P-08, P-10 i P-12 wynik należy opisywać tak:

> Najlepszy preset znaleziony dla jawnie podanej wersji symulatora, celu, profilu, konfiguracji i seedów.

Nie należy jeszcze pisać:

> Najlepszy preset dla mózgu człowieka, koncentracji, ADHD, medytacji albo snu.

Finalny raport optymalizacji powinien docelowo zawierać:

- wersję modelu i scoringu,
- pełną sygnaturę konfiguracji,
- liczbę prawdziwych ewaluacji,
- użyte seedy i długości sygnału,
- średni wynik i jego rozrzut,
- osobne wyniki akustyczne i neuronalne,
- status dostępności każdej metryki,
- wynik na holdoucie lub jasny napis, że holdout nie istnieje,
- poziom dowodów dla celu.
