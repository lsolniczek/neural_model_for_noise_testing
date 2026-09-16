# DSP — plan wersjonowanych seedów oraz API Swift i WASM

**Status:** `accepted`; wydano `v0.4.0`
**Data planu:** 2026-09-05; aktualizacja 2026-09-17
**Repozytorium docelowe:** noise_generator_dsp
**Audytowany commit DSP:** 20611c7e2b93e170657cda432f4faca41028f2fe
**Wydanie `v0.4.0`:** 81b51fad005bb6522cbbf42ef08ca4a3c6c9ab06
**Zależność:** plan jest niezależnym zadaniem DSP wykonywanym przed powrotem do P-06 w NMM
**Zakres dokumentu:** zamrożony kontrakt, kryteria odbioru i stan wdrożenia DSP

Wydanie implementuje opisany kontrakt w Rust, Swift i WASM. Pełny odbiór
2026-09-17 przeszedł 7/7: testy workspace, goldeny, alokacje, Swift/XCFramework,
WASM/Node, TypeScript oraz niezależny replay panelu 1 344 renderów. Obie pary
CPU przeszły za pierwszą próbą, a historyczne bramy CPU przeszły przy limicie
3%. Commit jest opublikowany na `master` i pod tagiem `v0.4.0`; NMM przypina go
po pełnym SHA.

## 1. Decyzja w skrócie

DSP dostanie nową, opcjonalną ścieżkę uruchomienia z seedem. Obecne konstruktory
i ich dźwięk pozostają bez zmian. Seed będzie wybierał konkretną realizację
szumu, ale nie zmieni koloru szumu, filtrów, poziomów, przestrzeni ani sposobu
budowania sceny binauralnej.

| Obszar | Decyzja |
|---|---|
| Publiczny seed | Jeden u64; wartość 0 jest poprawna |
| Swift | UInt64 oraz nazwane konstruktory seeded |
| WASM/JavaScript | bigint; dodatkowo wariant high/low u32 |
| Rozdzielanie strumieni | Wersjonowane drzewo oparte na BLAKE3 |
| Generator próbek | Zachowujemy obecny fastrand i SplitMix w liściach |
| Zgodność | Stare konstruktory i istniejące jawne seedy natury zachowują wynik |
| Zmiana seeda w locie | Nie; nowy seed wymaga utworzenia nowego silnika |
| Spread | Stałe współczynniki allpass; poza root_seed w V1 |
| Pogłos losujący topologię | W V1 pozostaje stałą częścią algorytmu |
| WASM | Pierwsze małe API JS do konstrukcji i renderu offline; nie pełne API realtime |
| NMM | Po wydaniu DSP P-06 wyprowadzi osobny u64 dla domeny audio i poda go do DSP |

Najważniejsza zasada: seed steruje **realizacją** szumu, a nie jego
neuroakustycznym znaczeniem. Nie wolno przy okazji zmienić relacji między
kanałami, niezależności obiektów ani charakteru anchoru.

### 1.1. Rozstrzygnięcia review planu

| Priorytet | Uwaga | Rozstrzygnięcie |
|---|---|---|
| P1 | KDF mógł trafić do callbacku przy przebudowie | SeedMaterialV1 dla MAX_SOURCES jest liczony tylko w konstruktorze |
| P1 | Golden jawnej natury mieszał generator ze spread/modulatorami | osobny golden legacy całej sceny i izolowany test generatora w seeded |
| P1 | Mean absolute value nie mierzył DC | brama używa abs(mean(x)) osobno dla L/R |
| P2 | Kolejność komend nie określa chwili zmiany | harmonogram zapisuje sample_offset i granice zdarzeń |
| P2 | wasm-bindgen konwertował przed walidacją | wejścia przechodzą przez JsValue/f64, potem TypeError/RangeError i cast |
| P2 | Klient nie znał rewizji seeded | instancja udostępnia effective_renderer_revision |
| P1 | Same granice zdarzeń nie określały bloków DSP | RenderScheduleV1 zapisuje pełny, kanoniczny podział na bloki |
| P1 | Absolutny brak alokacji rozszerzał zakres seedowania | przebudowa ma zero dodatkowych alokacji względem legacy baseline |
| P2 | Seeded spread zmieniał filtry allpass i przestrzeń | spread pozostaje stałą topologią poza root_seed w V1 |

## 2. Dlaczego robimy to jako osobne zadanie DSP

### Problem

DSP używa dziś wielu stałych seedów. Dwa silniki z taką samą konfiguracją dają
ten sam dźwięk, ale klient nie może wybrać innej, równie poprawnej realizacji.
NMM nie może więc rzetelnie sprawdzić presetu na wielu realizacjach szumu.

Gdyby seedy zostały dopięte wyłącznie po stronie NMM, część losowości nadal
pozostałaby ukryta w DSP. Trudniej byłoby też sprawdzić zgodność Swift, Rust i
WASM.

### Pomysł na rozwiązanie

Najpierw dodajemy i wydajemy samodzielny kontrakt seedów w DSP. Dopiero potem
NMM korzysta z gotowego, przetestowanego API. Dzięki temu błąd w generowaniu
audio da się oddzielić od błędu w eksperymencie NMM.

### Dlaczego warto

- aplikacja może odtworzyć konkretną realizację dźwięku;
- test może porównać dwa uruchomienia o tym samym seedzie;
- NMM może badać stabilność presetu na wielu realizacjach;
- Swift i WASM korzystają z tej samej semantyki;
- stara aplikacja nie musi zmieniać kodu ani brzmienia.

## 3. Stan bazowy i wdrożony kontrakt DSP

### 3.1. Stan bazowy Rust i silnika

- NoiseEngine ma konstruktory new(sample_rate, master_gain) oraz
  new_with_source_count(sample_rate, master_gain, source_count).
- Obiekty szumowe, anchor oraz modulatory startują ze stałych seedów.
- Zwykły obiekt przestrzenny ma jeden monofoniczny strumień szumu przed HRTF.
  Kanały L/R powstają z tego samego źródła przez przestrzeń, ITD i ILD.
- Anchor ma dwa osobne generatory L/R. Jest to obecna, zamierzona topologia.
- Źródła natury mają już jawne API
  set_object_source_nature(index, kind, seed).
- Zmiana liczby źródeł i zmiana sample rate potrafią przebudowywać stan.
  Seed musi być zachowany także w tych ścieżkach.
- Render realtime ma test braku alokacji i test pracy na stosie 2 MiB.
- Obecna rewizja renderera to dsp_brown_hf_v2.
- Zależność fastrand jest zadeklarowana szeroko jako wersja 2.

### 3.2. Stan bazowy Swift

- Obiekt Swift jest tworzony przez wygenerowane bindingi UniFFI.
- Render realtime idzie później przez istniejące C-FFI:
  noise_generator_engine_ptr i noise_generator_render_into.
- Dodanie seeda nie wymaga zmiany callbacku audio ani jego sygnatury.
- Skrypt build_xcframework.sh buduje urządzenie, symulator i macOS oraz generuje
  bindingi, ale walidacja powinna zacząć kompilować realne użycie nowego API.

### 3.3. Stan bazowy WASM

- Crate wasm obecnie tylko ponownie eksportuje API Rust.
- Zależność wasm-bindgen istnieje, ale nie ma jeszcze publicznego wrappera
  wywoływalnego z JavaScript.
- Dzisiejszy build potwierdza kompilację do wasm32, a nie działanie API JS.

Wniosek: nie należy opisywać tego zadania jako dodania seeda do istniejącego
pełnego API WASM. W ramach zadania powstanie pierwsza, celowo ograniczona
warstwa JavaScript.

### 3.4. Stan wydania `v0.4.0`

- Rust udostępnia `NoiseEngine::seeded(sample_rate, master_gain, seed: u64)`
  oraz `NoiseEngine::seeded_with_source_count(..., seed: u64)`.
- `SeedTreeV1` i materiał wszystkich trwałych slotów są liczone przy konstrukcji;
  render i przebudowy nie uruchamiają KDF.
- Anchor zachowuje wersjonowaną korelację `dsp_correlation_v1`: dwa liście KDF
  wybierają wspólną parę stanów, więc test zmiany seeda obejmuje oba kanały,
  ale nie deklaruje ich niezależności.
- Swift przyjmuje `UInt64`; WASM przyjmuje `bigint` i sprawdzony wariant dwóch
  części `u32`.
- Publiczne crate'y core, ios i wasm mają wersję `0.4.0`; zależności `fastrand`
  i `blake3` są przypięte odpowiednio do `2.3.0` i `1.8.2`.

## 4. Cele i rzeczy poza zakresem

### Cele

1. Ten sam seed, konfiguracja, pełny harmonogram bloków i komend, wersja DSP
   oraz target dają te same próbki.
2. Inne seedy zmieniają realizację wszystkich objętych kontraktem generatorów
   stochastycznych.
3. Dodanie lub wyłączenie obiektu nie zmienia strumienia innych slotów.
4. Obecne API i obecny dźwięk pozostają zgodne.
5. Swift i WASM udostępniają seed bez utraty bitów.
6. Metadane pozwalają zapisać seed i rewizję jego interpretacji.
7. Ścieżka steady-state realtime pozostaje bez alokacji i bez hashowania.
   Przebudowa source_count albo sample rate nie wykonuje KDF i nie dodaje
   alokacji ponad zachowanie legacy z audytowanego baseline'u.

### Poza zakresem

- dobieranie najlepszego seeda albo optymalizowanie seeda;
- zmiana koloru szumu, filtrów, HRTF, miksu lub modelu pogłosu;
- pełne odtworzenie całego API Swift w JavaScript;
- realtime AudioWorklet i współdzielony bufor WASM;
- setter zmieniający seed działającego silnika;
- automatyzacja sample-accurate niezależna od granic bloków renderu;
- kryptograficzne zastosowanie seeda;
- gwarancja identycznych bitów audio pomiędzy różnymi architekturami;
- zmiany w NMM. Integracja NMM nastąpi dopiero po zamknięciu tego planu.

## 5. Docelowy kontrakt seedów DSP

### 5.1. Dwa jawne tryby

Silnik przechowuje niezmienną politykę utworzoną w konstruktorze:

    enum SeedPolicy {
        LegacyFixedV1,
        RootSeededV1 { root_seed: u64 },
    }

- LegacyFixedV1 jest używany przez obecne konstruktory.
- RootSeededV1 jest używany wyłącznie przez nowe konstruktory.
- Nie istnieje specjalna wartość oznaczająca tryb legacy. Seed 0 jest zwykłym,
  poprawnym seedem.
- Nie ma set_seed. Zmiana seeda wymaga nowego NoiseEngine.

Brak settera jest celowy. Setter musiałby określić, czy resetuje fazy filtrów,
bufory opóźnień, pogłos, modulatory i źródła natury. Częściowy reset dawałby
niejasne wyniki i mógłby powodować kliknięcie w audio.

### 5.2. SeedTreeV1

Powstaje mały wewnętrzny moduł z typowanymi domenami. Miejsca wywołania nie
przekazują dowolnych napisów i nie wykonują root_seed + index.

Stały kontekst BLAKE3:

    com.noisegenerator.dsp.seed-tree.2026-09-05.v1

Kodowanie wejścia:

1. root_seed jako 8 bajtów little-endian;
2. ID domeny jako 8 bajtów little-endian;
3. liczba indeksów jako 8 bajtów little-endian;
4. każdy indeks jako 8 bajtów little-endian.

BLAKE3 derive_key zwraca 32 bajty. Pierwsze 8 bajtów little-endian tworzy u64
podawany do obecnego generatora liściowego. Zmiana kontekstu, kodowania, ID,
generatora lub liczby pobrań z generatora oznacza nową rewizję.

### 5.3. Zamrożone domeny V1

| ID | Domena | Indeksy | Znaczenie |
|---:|---|---|---|
| 1 | anchor_left | brak | generator lewego anchoru |
| 2 | anchor_right | brak | generator prawego anchoru |
| 3 | object_noise | object_slot | bazowy szum obiektu |
| 4 | object_bass_mod | object_slot | losowy modulator pasma bass |
| 5 | object_satellite_mod | object_slot | losowy modulator pasma satellite |
| 6 | object_nature | object_slot | odziedziczony seed źródła natury |

object_slot jest numerem trwałego miejsca w silniku, nie numerem na liście
aktywnych obiektów. Slot 3 zawsze ma adres object_noise/3 niezależnie od tego,
czy sloty 0–2 są aktywne.

ID 7–31 pozostają zarezerwowane. Raz wydanego ID nie wolno zmienić ani użyć do
innego celu.

### 5.4. Co obejmuje seed, a czego nie obejmuje w V1

Seed obejmuje:

- lewy i prawy generator anchoru;
- bazowy generator każdego slotu obiektu;
- bass i satellite stochastic/random-pulse modulators;
- źródło natury w nowym trybie inherited.

Seed nie zmienia w V1:

- parametrów, filtrów i trasowania;
- HRTF ani ruchu;
- częstotliwości i Q filtrów allpass procesora spread;
- stałych losowo wyglądających układów impulsów w velvet/sparse reverb;
- stałych topologii FDN;
- jawnego seeda przekazanego przez istniejące set_object_source_nature.

Losowe układy pogłosu pozostają stałą, wersjonowaną częścią algorytmu. Gdyby
zależały od root_seed, zmienialibyśmy jednocześnie realizację szumu i akustykę
pomieszczenia. Ewentualne losowanie pokoju powinno być oddzielnym przyszłym
parametrem.

Spread również pozostaje w V1 stałą, wersjonowaną topologią. Funkcja
spread_stage_filter losuje częstotliwość i Q filtrów allpass osobno dla L/R.
Podłączenie jej do root_seed zmieniałoby charakterystykę fazową dekorrelatora,
a więc przestrzeń, nie tylko realizację wejściowego szumu. Obecne stałe seedy
spreadu muszą zostać zachowane i objęte legacy/structural goldenem.

### 5.5. Generator liściowy

W V1 zachowujemy obecne fastrand::Rng oraz istniejące generatory SplitMix źródeł
natury. BLAKE3 służy tylko do otrzymania niezależnych seedów w publicznym
konstruktorze NoiseEngine. Nie wolno uruchamiać BLAKE3 przy późniejszej
przebudowie stanu.

To rozróżnienie jest ważne, ponieważ apply_source_count oraz przebudowa sample
rate są wykonywane przez render_into_internal na wątku audio. „Tworzenie
obiektu” wewnątrz tych metod nadal jest pracą callbacku.

Bezpieczny zakres zmiany:

- przypiąć bezpośrednią zależność fastrand do dokładnej audytowanej wersji
  2.3.0;
- przypiąć bezpośrednią zależność blake3 do jednej dokładnej wersji;
- dodać known-answer tests dla drzewa i generatora;
- potraktować zmianę wersji lub algorytmu RNG jako nową rewizję.

Nie wymieniamy teraz wszystkich generatorów na ChaCha. Taka zmiana miałaby
większy wpływ na kod realtime i brzmienie, a nie jest potrzebna do poprawnego
rozdzielenia strumieni.

### 5.6. Prekomputowany SeedMaterialV1

Konstruktor NoiseEngine oblicza raz i przechowuje stały zestaw liści dla
anchoru oraz **wszystkich MAX_SOURCES**, również nieaktywnych:

    struct ObjectSeedMaterialV1 {
        noise: u64,
        bass_mod: u64,
        satellite_mod: u64,
        nature: u64,
    }

    struct SeedMaterialV1 {
        anchor_left: u64,
        anchor_right: u64,
        objects: [ObjectSeedMaterialV1; MAX_SOURCES],
    }

NoiseEngine przechowuje SeedPolicy i gotowy SeedMaterialV1 poza mutowanym
stanem renderera. LegacyFixedV1 również dostaje materiał złożony z obecnych
stałych, ale bez uruchamiania BLAKE3.

apply_source_count, recalculate_sample_rate, SpatialObject::create,
AnchorState::new oraz wszystkie ścieżki odtwarzające generatory przyjmują
gotowe u64. Nie mają dostępu do SeedTreeV1 ani root_seed.

Wymaganie architektoniczne: moduł SeedTreeV1 może być wywołany tylko z
konstruktorów publicznych. Testowa sonda licząca wywołania KDF musi potwierdzić
zero nowych wywołań podczas renderu, w tym pierwszego renderu po zmianie
source_count i sample rate.

## 6. Kontrakt neuroakustyczny i korelacja

Dodanie seeda nie może przypadkiem zmienić obrazu przestrzennego.

### 6.1. Zwykły obiekt przestrzenny

Jeden obiekt nadal generuje jeden sygnał mono, a dopiero potem przechodzi przez
HRTF/spatializer do L/R. Nie tworzymy osobnego RNG na lewe i prawe ucho dla
tego samego punktowego źródła. Dwa niezależne RNG zmieniłyby spójność
międzyuszną i mogłyby poszerzyć lub rozmyć źródło.

### 6.2. Anchor

Anchor zachowuje obecną konstrukcję z dwoma niezależnymi strumieniami L/R.
Nowe domeny anchor_left i anchor_right tylko pozwalają wybrać ich realizację.
Nie wolno podać obu kanałom tego samego seeda ani zastąpić ich jednym
strumieniem mono.

### 6.3. Obiekty i modulatory

- różne sloty obiektów mają osobne strumienie;
- bass i satellite mają osobne domeny;
- kolejność utworzenia, liczba aktywnych obiektów i liczba wątków nie mogą
  przesuwać globalnego RNG;
- root_seed jest zmienną techniczną eksperymentu, nie parametrem presetu i nie
  cechą optymalizowaną przez NMM.

Kontrakt otrzymuje nazwę dsp_correlation_v1. Jego zmiana wymaga osobnego review
neuroakustycznego, nowych statystyk i jawnej zmiany rewizji.

## 7. API Rust i UniFFI

### 7.1. Istniejące API pozostaje

    NoiseEngine::new(sample_rate: u32, master_gain: f32)
    NoiseEngine::new_with_source_count(
        sample_rate: u32,
        master_gain: f32,
        source_count: u32,
    )

Oba konstruktory tworzą LegacyFixedV1 i muszą przejść istniejące goldeny bez
zmiany próbek.

### 7.2. Nowe nazwane konstruktory

Docelowa semantyka Rust:

    NoiseEngine::seeded(
        sample_rate: u32,
        master_gain: f32,
        seed: u64,
    )

    NoiseEngine::seeded_with_source_count(
        sample_rate: u32,
        master_gain: f32,
        source_count: u32,
        seed: u64,
    )

Metody są eksportowane jako nazwane konstruktory UniFFI. Nie dokładamy seeda
opcjonalnego do starego konstruktora, bo zmieniłoby to wygenerowane API i
utrudniło odróżnienie trybu legacy.

### 7.3. Źródła natury

Istniejące:

    set_object_source_nature(index, kind, seed)

pozostaje dokładnym jawnym override. Seed 0 zachowuje znaczenie prawidłowej
wartości, a nie hasła „użyj seeda silnika”.

Nowe API:

    set_object_source_nature_inherited(index, kind)

wyprowadza object_nature/object_slot z root_seed. Dla silnika legacy używa
udokumentowanego legacy seed dla danego slotu. Nie zmieniamy po cichu semantyki
starej metody.

### 7.4. Metadane

API tylko do odczytu:

    seed_mode() -> SeedMode
    root_seed() -> Option<u64>
    effective_renderer_revision() -> String
    base_renderer_revision() -> String
    seed_derivation_revision() -> String
    correlation_policy_revision() -> String

SeedMode ma stabilne wartości LegacyFixedV1 i RootSeededV1. Metadane nie są
odczytywane w callbacku audio. effective_renderer_revision zwraca rewizję
właściwą dla danej instancji: legacy albo seeded. Klient nie musi zgadywać jej
na podstawie seed_mode.

Warstwa C-FFI używana przez klientów Swift dostaje wyłącznie nową funkcję
sterującą dla trybu inherited:

    noise_generator_set_object_source_nature_inherited(
        engine_ptr,
        index,
        kind
    )

Nie dodajemy konstruktorów C. Utworzenie i czas życia obiektu nadal obsługuje
UniFFI, a C-FFI pozostaje cienką warstwą realtime/control.

## 8. API Swift

### 8.1. Oczekiwany interfejs

Stare inicjalizatory nadal działają:

    let legacy = NoiseEngine(sampleRate: 48_000, masterGain: 0.8)
    let legacy4 = NoiseEngine(
        sampleRate: 48_000,
        masterGain: 0.8,
        sourceCount: 4
    )

Nowe nazwane konstruktory/fabryki wygenerowane przez UniFFI mają dać
jednoznaczne wywołania:

    let seeded = NoiseEngine.seeded(
        sampleRate: 48_000,
        masterGain: 0.8,
        seed: UInt64(42)
    )

    let seeded4 = NoiseEngine.seededWithSourceCount(
        sampleRate: 48_000,
        masterGain: 0.8,
        sourceCount: 4,
        seed: UInt64(42)
    )

Dokładna pisownia wygenerowanego symbolu zostaje zamrożona testem snapshot i
przykładem kompilowanym przez Xcode. Jeżeli aktualna wersja UniFFI wygeneruje
inną nazwę, implementacja dodaje cienki, ręczny adapter Swift o nazwach
powyżej, zamiast zmieniać kontrakt dokumentu.

Dodatkowo Swift udostępnia:

    engine.seedMode()
    engine.rootSeed()                 // UInt64?
    engine.effectiveRendererRevision()
    engine.seedDerivationRevision()
    engine.correlationPolicyRevision()
    engine.setObjectSourceNatureInherited(index: 0, kind: .rain)

### 8.2. Realtime i ABI

- noise_generator_engine_ptr pozostaje bez zmiany;
- noise_generator_render_into pozostaje bez zmiany;
- seed jest używany przed startem renderu;
- nie dodajemy seeda do każdej funkcji renderującej;
- publiczny nagłówek C ma przejść test różnicy ABI;
- wygenerowany Swift ma używać UInt64, nigdy Int ani Double.

## 9. API WASM/JavaScript

### 9.1. Uczciwy zakres V1

Crate zachowuje ponowny eksport core dla konsumentów Rust i dodaje wrapper
WasmNoiseEngine oznaczony wasm_bindgen. Jest to małe API do testów,
reprodukcji i renderu offline. Nie jest to jeszcze callback realtime ani pełna
parytetowa warstwa sterowania.

### 9.2. Oczekiwany interfejs JavaScript/TypeScript

    const legacy = new NoiseEngine(48_000, 0.8);
    const legacy4 = NoiseEngine.withSourceCount(48_000, 0.8, 4);
    const seeded = NoiseEngine.seeded(48_000, 0.8, 42n);
    const seeded4 =
        NoiseEngine.seededWithSourceCount(48_000, 0.8, 4, 42n);
    const seededParts =
        NoiseEngine.seededWithSourceCountParts(
            48_000,
            0.8,
            4,
            0,
            42
        );

    const stereo: Float32Array = seeded.renderAudio(256);

Kontrakt:

- u64 w wasm-bindgen jest wystawione jako JavaScript bigint;
- API nie przyjmuje seeda jako number, ponieważ number nie zachowuje wszystkich
  64 bitów;
- wariant Parts składa seed dokładnie jako
  (BigInt(seedHigh) << 32n) | BigInt(seedLow);
- pełny seed oraz wszystkie wejścia całkowite (sampleRate, sourceCount,
  frameCount, seedHigh i seedLow) przekraczają surową granicę wasm-bindgen jako
  JsValue albo f64; nie jako u64/u32;
- publiczny adapter sprawdza typ i zakres, a dopiero potem konwertuje do
  u64/u32;
- seedHigh i seedLow muszą być skończonymi liczbami całkowitymi z zakresu
  0..=4_294_967_295;
- sampleRate musi być skończoną liczbą całkowitą z zakresu 8_000..=192_000;
- sourceCount musi być skończoną liczbą całkowitą mieszczącą się w u32; po
  walidacji zachowuje istniejącą semantykę clamp core;
- pełny seed musi być typu bigint z zakresu 0n..=18_446_744_073_709_551_615n;
- renderAudio(frameCount) zwraca przeplatany Float32Array L/R o długości
  frameCount * 2;
- frameCount musi być skończoną liczbą całkowitą z zakresu
  1..=MAX_WASM_RENDER_FRAMES;
- MAX_WASM_RENDER_FRAMES wynosi w V1 dokładnie 262_144, czyli maksymalnie
  2 MiB bufora stereo f32 na jedno wywołanie;
- błędny typ, zakres albo frameCount zwraca opisany JavaScript RangeError lub
  TypeError przed alokacją i przed konwersją do typu Rust;
- seed 0n i części 0/0 są poprawne.

Jest to wymaganie warstwy publicznej, ponieważ automatyczna konwersja
wasm-bindgen zaokrągla liczby w kierunku zera, zamienia NaN na zero i zawija
wartości spoza zakresu. BigInt spoza zakresu u64 także jest zawijany. Sam typ
argumentu Rust nie jest więc walidacją.

Tabela błędów publicznego API:

| Przypadek | Wynik |
|---|---|
| seed nie jest bigint | TypeError |
| seed bigint jest ujemny lub większy od u64::MAX | RangeError |
| wejście całkowite nie jest number | TypeError |
| wejście całkowite jest NaN/infinity/ułamkiem | RangeError |
| high lub low jest poza zakresem u32 | RangeError |
| sampleRate jest poza zakresem 8_000..=192_000 | RangeError |
| sourceCount jest ujemny lub większy od u32::MAX | RangeError |
| frameCount jest poza zakresem 1..=262_144 | RangeError |

Wygenerowany surowy binding nie jest bezpośrednio eksportowany jako publiczne
API pakietu. Cienki adapter JS/TS albo metoda przyjmująca JsValue wykonuje
walidację przed pierwszym castem. Deklaracja TypeScript nadal wystawia bigint
i number, a nie any.

Metadane:

    seeded.seedMode()
    seeded.rootSeed()                 // bigint | undefined
    seeded.effectiveRendererRevision()
    seeded.seedDerivationRevision()
    seeded.correlationPolicyRevision()
    NoiseEngine.baseRendererRevision()

seedMode zwraca dokładnie jeden z napisów legacy_fixed_v1 albo
root_seeded_v1. Deklaracja TypeScript używa unii tych dwóch literałów, a nie
dowolnego string.

### 9.3. Świadome ograniczenie

renderAudio może w V1 skopiować wynik do nowego Float32Array. Jest przeznaczone
do testów i renderu offline. API bez alokacji oparte na pamięci liniowej i
AudioWorklet jest osobnym zadaniem; nie blokuje P-06, które używa core Rust.

## 10. Stan, przebudowa i stabilność adresów

Implementacja musi przeprowadzić SeedPolicy przez wszystkie miejsca tworzenia
stanu, nie tylko przez główny konstruktor.

| Zdarzenie | Wymagane zachowanie |
|---|---|
| Utworzenie silnika | Wszystkie liście dla MAX_SOURCES obliczone z polityki |
| Większy source_count | Stary i nowy slot używają wcześniej zapisanych u64; brak KDF |
| Aktywacja/dezaktywacja | Nie reseeduje innych slotów |
| Zmiana sample rate | Przebudowa używa zapisanych u64; brak KDF w renderze |
| Ten sam harmonogram | Te same komendy i identyczny pełny podział na bloki dają ten sam wynik |
| Jawna natura | Używa dokładnie przekazanego seeda |
| Natura inherited | Używa domeny obiektu z root_seed |

Nie wolno przechowywać jednego globalnego RNG, z którego kolejne komponenty
pobierają seedy. Taki projekt uzależniłby wynik od kolejności inicjalizacji.

### 10.1. RenderScheduleV1

Obecny silnik pobiera część parametrów na początku bloku, dlatego „ta sama
kolejność setterów” ani same granice zdarzeń nie wystarczają do określenia
identycznego audio. LateInputRoutingState może po wygaszeniu czekać w
WaitingForBlockSwap na następny początek bloku. Długość bloku po zdarzeniu
wpływa więc na moment przełączenia.

Gwarancja V1:

- przy stałej konfiguracji dowolny podział renderu na bloki daje to samo audio;
- automatyzacja jest opisana przez RenderScheduleV1;
- harmonogram zawiera total_frames, bazowy rozmiar bloku 256,
  uporządkowane komendy (sample_offset, sequence_number, command, arguments)
  oraz pełną listę przedziałów renderu [start, end);
- kanoniczne granice są posortowaną unią 0, total_frames, wielokrotności 256
  liczone od offsetu 0 oraz wszystkich sample_offset komend;
- sąsiednie kanoniczne granice tworzą dokładnie jedno wywołanie render_into;
- komendy dla danego offsetu są wykonywane według sequence_number przed
  blokiem zaczynającym się na tym offsecie;
- setter jest wywołany przed blokiem zaczynającym się na wskazanym offsecie;
- klient może dostarczać inne zewnętrzne porcje danych tylko wtedy, gdy
  dispatcher je buforuje i wywołuje DSP według identycznych kanonicznych
  przedziałów;
- manifest zapisuje pełną listę przedziałów, a nie tylko offsety zdarzeń;
- bez dispatchera dwa uruchomienia muszą użyć identycznego pełnego podziału
  wywołań render_into;
- nie gwarantujemy tego samego audio przy innym podziale bloków automatyzacji,
  nawet jeżeli offsety i kolejność setterów są takie same.

Szersza niezależność automatyzacji od granic bloków wymagałaby kolejki zdarzeń
sample-accurate i jest osobnym zadaniem, nie częścią seedowania.

Wartość zapisywana w manifeście dla tego algorytmu to
dsp_render_schedule_v1. Zmiana bazowego rozmiaru bloku, sposobu tworzenia
granic albo kolejności komend oznacza nową rewizję harmonogramu.

## 11. Rewizje i zgodność wersji

Dodajemy:

    SEED_DERIVATION_REVISION = "dsp_seed_tree_v1"
    CORRELATION_POLICY_REVISION = "dsp_correlation_v1"
    SEEDED_RENDERER_REVISION = "dsp_brown_hf_v2_seeded_v1"

Istniejący RENDERER_REVISION pozostaje dsp_brown_hf_v2, jeżeli goldeny starych
konstruktorów nie zmienią ani jednej próbki. Samo dodanie opcjonalnej ścieżki
nie jest powodem do zmiany rewizji legacy.

Instancja LegacyFixedV1 zwraca z effective_renderer_revision wartość
dsp_brown_hf_v2. Instancja RootSeededV1 zwraca
dsp_brown_hf_v2_seeded_v1. Statyczne base_renderer_revision opisuje wyłącznie
wspólny bazowy algorytm i nie może zastąpić gettera instancji w manifeście.

Jeżeli stare próbki się zmienią, implementacja ma się zatrzymać i wyjaśnić
przyczynę. Nie aktualizujemy goldenów automatycznie. Zmiana legacy wymaga
osobnej decyzji i migracji.

Wydanie z nowym publicznym API powinno zwiększyć wersję minor crate'ów, np.
0.1.x do 0.2.0, zgodnie z polityką wersjonowania repozytorium.

## 12. Plan implementacji

### Faza 0 — zamrożenie punktu odniesienia

1. Zapisać commit, toolchain, Cargo.lock i rewizję renderera.
2. Uruchomić pełny zestaw testów przed zmianą.
3. Zapisać goldeny starych konstruktorów dla reprezentatywnych scen.
4. Zrobić pełny inwentarz wszystkich fastrand, SplitMix i stałych seedów.
5. Oznaczyć każdy wpis jako:
   - realizacja sygnału;
   - korelacja/topologia sceny;
   - stała topologia algorytmu;
   - dane testowe.
6. Zapisać bazowy benchmark CPU i statystyki neuroakustyczne.
7. Zmierzyć liczbę alokacji i bajtów w callbacku stosującym osobno zwiększenie
   source_count i zmianę sample rate.

**Brama:** żadna implementacja nie zaczyna się, dopóki inwentarz nie obejmuje
każdego wywołania tworzącego RNG w kodzie produkcyjnym.

### Faza 1 — rdzeń drzewa seedów

1. Dodać dokładnie przypięte zależności blake3 i fastrand.
2. Dodać typy SeedPolicy, SeedDomain i SeedTreeV1.
3. Zamrozić tabelę domen i format bajtowy.
4. Dodać known-answer tests dla seedów 0, 1, 42 i u64::MAX.
5. Dodać test znanego ciągu fastrand dla jednego liścia.
6. Dodać SeedMaterialV1 zawierający liście dla anchoru i wszystkich
   MAX_SOURCES.
7. Ograniczyć wywołanie hash/KDF do publicznych konstruktorów NoiseEngine.

**Brama:** known-answer tests przechodzą natywnie i na wasm32.

### Faza 2 — podłączenie silnika

1. Zostawić stare konstruktory na LegacyFixedV1.
2. Dodać oba nazwane konstruktory seeded.
3. Przekazać gotowy SeedMaterialV1 do NoiseEngineInner, SpatialObject,
   AnchorState i modulatorów; te komponenty nie wyprowadzają liści.
4. Obsłużyć rozszerzanie liczby źródeł i przebudowę sample rate.
5. Nie zmieniać kolejności renderu ani liczby losowań na próbkę.
6. Nie podłączać do root_seed losowej topologii pogłosu.
7. Zachować stałe seedy i współczynniki procesora spread niezależnie od
   root_seed.
8. Dodać sondę/test potwierdzający brak KDF po zmianie source_count i sample
   rate wykonywanej przy następnym renderze.

**Brama:** wszystkie goldeny legacy są bitowo identyczne.

### Faza 3 — natura i metadane

1. Zachować stare jawne API natury.
2. Dodać set_object_source_nature_inherited.
3. Dodać gettery trybu, seeda, efektywnej rewizji renderera i pozostałych
   rewizji.
4. Udokumentować regułę override kontra inherited.
5. Dodać testy wartości 0 oraz maksymalnego u64.

**Brama:** w legacy jawny seed natury zachowuje stary golden całej sceny.
W seeded wyjście samego generatora natury z jawnym seedem nie zależy od
root_seed, natomiast końcowe audio może się różnić przez seeded modulatory.
inherited reaguje na root_seed.

### Faza 4 — testy DSP i neuroakustyczne

1. Dodać scenariusze golden dla każdej domeny.
2. Dodać testy izolacji slotów, niezmienności rozmiaru bloku przy stałej
   konfiguracji oraz RenderScheduleV1 z pełną listą bloków.
3. Dodać panel 64 seedów z metrykami rozkładu i korelacji.
4. Ponownie uruchomić realtime allocation oraz test stosu 2 MiB.
5. Porównać CPU starego i nowego konstruktora.

**Brama:** wszystkie kryteria z sekcji 13.1–13.3 są spełnione.

### Faza 5 — Swift

1. Wyeksportować nazwane konstruktory i metadane przez UniFFI.
2. Wygenerować bindingi i zamrozić nazwy publiczne.
3. Dodać kompilowany SeededApiSmoke.swift do projektu walidacyjnego.
4. W smoke teście utworzyć legacy i seeded, pobrać pointer i wywołać
   noise_generator_render_into.
5. Dodać test runtime na macOS dla tego samego i innego seeda.
6. Sprawdzić effectiveRendererRevision dla legacy i seeded.
7. Zbudować XCFramework dla wszystkich obecnie wspieranych slice'ów.

**Brama:** stare użycie Swift nadal się kompiluje, nowe używa UInt64, a
sygnatury realtime C są bez zmian.

### Faza 6 — WASM

1. Dodać cienki WasmNoiseEngine bez duplikowania logiki core.
2. Dodać fabryki legacy, seeded, source count i parts.
3. Dodać publiczną walidację JsValue/f64 przed konwersją na u64/u32.
4. Dodać limit MAX_WASM_RENDER_FRAMES i renderAudio zwracające Float32Array.
5. Dodać negatywne testy TypeError/RangeError dla ułamków, liczb ujemnych,
   NaN, infinity, niepoprawnego sampleRate, przepełnień BigInt/u32 i
   nadmiernej alokacji.
6. Dodać wasm-bindgen-test uruchamiany w Node.
7. Dodać wygenerowanie pakietu oraz import smoke.
8. Sprawdzić deklaracje TypeScript przez tsc --noEmit.
9. Sprawdzić effectiveRendererRevision dla legacy i seeded.
10. Zachować test kompilacji istniejącego re-exportu Rust.

**Brama:** testy BigInt i Parts dają identyczny seed i identyczne audio w tym
samym środowisku.

### Faza 7 — wydanie i przekazanie do NMM

1. Uzupełnić README Rust, Swift, WASM i tabelę zgodności.
2. Dodać wpis changelog oraz wersję crate'ów.
3. Rozszerzyć validate_release o Swift smoke i Node WASM.
4. Uruchomić pełne CI z Cargo.lock.
5. Zapisać commit/tag DSP, rewizje i manifest goldenów.
6. Dopiero wtedy zmienić plan P-06 NMM tak, aby domena audio kończyła się
   u64 i korzystała z nowego konstruktora DSP.

**Brama:** NMM dostaje konkretny commit DSP i nie odwołuje się do API z
nieopublikowanej gałęzi.

## 13. Plan testów

### 13.1. Determinizm i rozdzielenie domen

| Test | Warunek zaliczenia |
|---|---|
| Known-answer SeedTree | Dokładne u64 dla wszystkich domen, root 0/1/42/MAX i slotów 0/1/7 |
| Powtórzenie | Dwa nowe silniki z tym samym seedem i pełnym RenderScheduleV1 mają identyczny hash |
| Inny seed | Każda izolowana domena stochastyczna zmienia hash dla seedów 42 i 43 |
| Rozdzielenie domen | Żadne dwa różne adresy z tabeli testowej nie dają tego samego liścia |
| Sloty | Liść i surowy strumień RNG slotu 0 nie zmieniają się przy większym source_count |
| Kolejność | Przy takim samym offsecie aktywacji kolejność innych slotów nie zmienia własnego strumienia badanego slotu |
| Bloki bez automatyzacji | Przy stałej konfiguracji rozmiary 1, 64, 127, 256 i 1024 dają po sklejeniu ten sam hash |
| Automatyzacja bez dispatchera | Identyczny hash wymaga identycznej pełnej listy wywołań render_into i komend |
| Automatyzacja z dispatcherem | Różne porcje klienta dają ten sam hash tylko gdy DSP otrzymuje identyczne kanoniczne bloki RenderScheduleV1 |
| LateInputRoutingState | Scenariusz obejmujący WaitingForBlockSwap przełącza stan na tej samej kanonicznej granicy |
| Przebudowa | Dwa silniki z takim samym pełnym harmonogramem bloków i zmian sample rate dają ten sam hash |
| Brak KDF w renderze | Licznik KDF nie rośnie przy zwykłym renderze ani pierwszym renderze po zmianie source_count/sample rate |
| Jawna natura legacy | Istniejący golden całej sceny z jawnym seedem pozostaje bez zmiany |
| Jawna natura seeded | Wyjście generatora natury albo izolowana scena bez innych seeded domen nie zależy od root_seed |
| Natura inherited | Ten sam root daje ten sam wynik; inny root daje inny wynik |
| Stały spread | Liście/współczynniki allpass spread są identyczne dla różnych root_seed |
| fastrand KAT | Przypięta wersja daje zamrożone pierwsze wartości dla znanego seeda |

Testy same/different nie mogą porównywać tylko metadanych. Muszą renderować
próbki.

### 13.2. Zgodność legacy

Stare konstruktory muszą zachować bitowo:

- scenę tylko z anchorem;
- biały, różowy i brązowy szum obiektu;
- stochastic bass i satellite;
- spread;
- reprezentatywne źródła natury z jawnym seedem;
- scenę łączącą wszystkie powyższe elementy.

Warunek zaliczenia: wszystkie istniejące i nowe legacy SHA-256 są identyczne z
manifestem fazy 0. Różnica jednej próbki blokuje wydanie.

### 13.3. Neuroakustyka i statystyka

Stały panel: 64 root seedy. Dla scen statystycznych renderujemy 60 sekund po
2 sekundach warm-up przy 48 kHz. Panel i konfiguracje są zapisane w manifeście.

Minimalne bramy:

- składowa stała abs(mean(x)) nie przekracza 0.001 FS, liczona osobno dla L
  i R; kanałów nie wolno uśredniać razem;
- średni RMS panelu nie odchyla się od baseline'u fazy 0 o więcej niż 0.25 dB;
- energia w każdym zapisanym paśmie 1/3 oktawy nie odchyla się od baseline'u
  o więcej niż 0.5 dB;
- nachylenie widma nie odchyla się o więcej niż 0.25 dB/oktawę;
- true peak nie przekracza istniejącego limitu bezpieczeństwa sceny;
- anchor zachowuje osobne liście seedów L/R, a jego IACC nie odchyla się od
  baseline'u o więcej niż 0.05;
- punktowy obiekt nadal ma jeden generator przed HRTF; test strukturalny
  blokuje przypadkowe dodanie niezależnego RNG na ucho;
- surowe, niezależne liście object_noise dla dwóch slotów mają
  |korelacja Pearsona| poniżej 0.01 dla 2^20 próbek;
- żaden wynik nie zawiera NaN ani infinity.

Baseline i progi są zamrażane przed podłączeniem seedów. Progu nie wolno
poluzować tylko dlatego, że nowa implementacja go nie przeszła; zmiana wymaga
opisu przyczyny i review DSP/neuroakustycznego.

### 13.4. Realtime i wydajność

- steady-state noise_generator_render_into bez oczekujących przebudów ma zero
  alokacji dla legacy i seeded;
- test stosu 2 MiB przechodzi;
- steady-state render loop nie ma BLAKE3, blokady ani alokacji;
- callback stosujący zmianę source_count albo sample rate może zachować
  istniejące alokacje legacy wynikające z Box/Vec;
- przed implementacją zapisujemy osobno liczbę alokacji i przydzielone bajty
  legacy dla obu przebudów na audytowanym commicie;
- seeded przebudowa nie może wykonać ani jednej alokacji i przydzielić ani
  jednego bajtu więcej niż odpowiadający jej legacy baseline;
- testowa sonda KDF pozostaje na zero po obu przebudowach;
- mediana CPU reprezentatywnego benchmarku seeded nie jest gorsza o więcej niż
  5% od legacy na tym samym commicie;
- obecne bramy CPU repozytorium nadal przechodzą;
- rozmiar NoiseEngine i pamięć na źródło są zapisane przed/po zmianie.

Regresja powyżej 5% albo dodatkowa alokacja podczas przebudowy wymaga profilu i
decyzji, nie automatycznego podniesienia limitu. Usunięcie istniejących
alokacji Box/Vec z przebudowy jest wartościowym osobnym zadaniem realtime, ale
nie blokuje seedowania.

### 13.5. Swift

- test snapshot potwierdza dokładne nazwy publiczne;
- seed jest UInt64 i obsługuje 0 oraz UInt64.max;
- stary kod inicjalizujący kompiluje się bez zmian;
- SeededApiSmoke.swift tworzy oba tryby, pobiera pointer i renderuje;
- macOS runtime smoke: same seed = ten sam hash, inny seed = inny hash;
- effectiveRendererRevision zwraca rewizję legacy dla starego konstruktora i
  seeded dla nowego;
- XCFramework zawiera obecne slice'y device, simulator i macOS;
- snapshot nagłówka C potwierdza brak zmian w noise_generator_engine_ptr i
  noise_generator_render_into;
- Archive/Release z włączonymi aktualnymi ustawieniami repozytorium przechodzi.

### 13.6. WASM

- cargo build --release --target wasm32-unknown-unknown przechodzi;
- wasm-bindgen-test działa w Node;
- 42n daje ten sam wynik co high=0, low=42;
- u64::MAX daje ten sam wynik co high=u32::MAX, low=u32::MAX;
- seed 0n jest przyjmowany;
- ujemny BigInt i BigInt większy od u64::MAX zwracają RangeError;
- seed podany jako number zamiast bigint zwraca TypeError;
- high/low o złym typie zwracają TypeError, a ułamek, liczba ujemna, NaN,
  infinity albo wartość większa od u32::MAX zwracają RangeError;
- sampleRate/sourceCount o złym typie zwracają TypeError, a wartości
  nieskończone, ułamkowe, ujemne lub poza udokumentowanym zakresem zwracają
  RangeError;
- frameCount równy 0, ułamkowy, ujemny, NaN, infinity lub większy od 262_144
  zwraca błąd przed alokacją;
- dwa silniki z tym samym seedem renderują identyczny Float32Array;
- seed 42n i 43n dają inny hash dla sceny stochastycznej;
- długość bufora wynosi frameCount * 2 i wszystkie próbki są skończone;
- TypeScript deklaruje bigint, a test celowo odrzuca number jako seed;
- pakiet można zaimportować i wywołać z czystego skryptu Node;
- panic Rust nie przekracza granicy jako nieopisany błąd JS.
- effectiveRendererRevision zwraca właściwą rewizję dla obu trybów.

Nie wymagamy identycznego hasha audio pomiędzy native, Swift i WASM, ponieważ
różne targety mogą różnić się szczegółami obliczeń zmiennoprzecinkowych.
Wymagamy identycznych known-answer values drzewa seedów oraz deterministycznego
audio w obrębie każdego wspieranego targetu.

### 13.7. Pełna regresja

- cargo fmt --check;
- cargo clippy dla workspace i wszystkich targetów używanych w CI;
- cargo test --workspace --locked;
- testy z aktualnymi feature flags oraz no-default-features, jeżeli wspierane;
- obecne testy regresji, natury, CPU i realtime;
- build XCFramework i jego walidacja;
- build oraz test Node WASM;
- dokumentacja nie obiecuje pełnej parytetowości WASM.

## 14. Definition of Done

Zadanie można oznaczyć jako implemented dopiero, gdy wszystkie pola są
spełnione.

### Kontrakt

- [x] Istnieje kompletny inwentarz RNG z klasyfikacją każdego generatora.
- [x] SeedTreeV1 ma zamrożony kontekst, format bajtów, ID i known-answer tests.
- [x] Konstruktor prekomputuje SeedMaterialV1 dla anchoru i wszystkich
      MAX_SOURCES.
- [x] Seed 0 jest prawidłowy i nie oznacza trybu legacy.
- [x] Nie ma arytmetyki root_seed + index ani globalnego RNG przydzielającego
      kolejne strumienie.
- [x] Wersje fastrand i blake3 są przypięte dokładnie.
- [x] Metadane zwracają seed mode, root seed, efektywną rewizję renderera i
      pozostałe rewizje.

### Dźwięk i zgodność

- [x] Obecne konstruktory dają bitowo te same goldeny.
- [x] Ten sam seed i pełny RenderScheduleV1 dają ten sam hash w obrębie
      targetu.
- [x] Manifest zapisuje wszystkie kanoniczne przedziały render_into i kolejność
      komend na wspólnym offsecie.
- [x] Test WaitingForBlockSwap potwierdza tę samą zmianę stanu na tej samej
      kanonicznej granicy bloku.
- [x] Inny seed zmienia każdą objętą kontraktem domenę stochastyczną.
- [x] Sloty są niezależne od liczby i kolejności aktywacji obiektów.
- [x] Zmiana sample rate nie gubi polityki seedów.
- [x] Jawny seed natury zachowuje legacy golden całej sceny.
- [x] W seeded jawny generator natury jest niezależny od root_seed, gdy
      pozostałe seeded domeny są wyłączone lub badane przed ich przetwarzaniem.
- [x] Nowy tryb inherited natury korzysta z drzewa.
- [x] Spread nie zależy od root_seed; jego współczynniki allpass zachowują
      structural golden.
- [x] Topologia mono-przed-HRTF i osobne L/R anchoru są zachowane.
- [x] Panel 64 seedów przechodzi zamrożone bramy neuroakustyczne.

### Realtime

- [x] Hashowanie odbywa się tylko poza render loop.
- [x] Wszystkie liście są gotowe przed pierwszym renderem.
- [x] Zmiana source_count i sample rate w callbacku używa gotowych u64 i nie
      zwiększa licznika KDF.
- [x] Render steady-state legacy i seeded nie alokuje.
- [x] Legacy baseline zapisuje liczbę i bajty alokacji przebudowy source_count
      oraz sample rate.
- [x] Seeded nie dodaje żadnej alokacji ani bajtu do odpowiadającego baseline'u
      przebudowy legacy.
- [x] Test stosu 2 MiB przechodzi.
- [x] Regresja mediany CPU nie przekracza 5%.

### Swift

- [x] Stare inicjalizatory nadal się kompilują.
- [x] Publiczne seeded i seededWithSourceCount mają stabilne nazwy.
- [x] Seed w Swift ma typ UInt64.
- [x] Smoke test kompiluje, linkuje i renderuje przez istniejące C-FFI.
- [x] Runtime smoke potwierdza same/different seed.
- [x] effectiveRendererRevision zwraca właściwą wartość dla legacy i seeded.
- [x] Wszystkie wymagane slice'y XCFramework są zbudowane.
- [x] Sygnatury realtime C są niezmienione.

### WASM

- [x] Istnieje pierwszy jawny wrapper JavaScript, opisany jako ograniczony.
- [x] Seed ma typ bigint; wariant u32 high/low daje identyczny wynik.
- [x] Walidacja typu, skończoności, całkowitości i zakresu zachodzi przed
      konwersją wasm-bindgen/Rust.
- [x] Ujemne, ułamkowe, NaN, infinity i przepełnione wejścia przechodzą
      negatywne testy TypeError/RangeError.
- [x] MAX_WASM_RENDER_FRAMES=262_144 blokuje nadmierną alokację.
- [x] renderAudio zwraca poprawny Float32Array stereo.
- [x] Testy Node same/different/zero/MAX przechodzą.
- [x] TypeScript i import smoke przechodzą.
- [x] Istniejący konsument Rust re-exportu nadal się kompiluje.

### Wydanie i przekazanie

- [x] Pełne CI DSP przechodzi z Cargo.lock.
- [x] README, przykłady, changelog i wersje crate'ów są aktualne.
- [x] Manifest zawiera commit DSP, toolchain, efektywną rewizję renderera,
      pozostałe rewizje, seedy, pełne RenderScheduleV1, sceny i hashe.
- [x] Opublikowany commit/tag DSP jest zapisany jako zależność NMM.
- [x] Dopiero po tym P-06 NMM zostaje zaktualizowane do finalnego API u64.

## 15. Pliki DSP, których prawdopodobnie dotknie implementacja

| Obszar | Pliki lub katalogi |
|---|---|
| Core i API | crates/core/src/lib.rs oraz nowy moduł seedów |
| Generatory/modulatory | crates/signal_core, crates/engine_shared |
| Zależności | Cargo.toml, Cargo.lock i Cargo.toml odpowiednich crate'ów |
| Swift/C-FFI | crates/ios/src/lib.rs, konfiguracja UniFFI, build_xcframework.sh |
| Swift validation | validation/ios-release |
| WASM | crates/wasm/src/lib.rs, Cargo.toml i testy Node/wasm-bindgen |
| Goldeny | crates/core/src/regression.rs i wersjonowany manifest |
| CI/release | istniejące workflow oraz validate_release |
| Dokumentacja | README.md i changelog |

Lista jest mapą startową, nie zgodą na mechaniczne zmienianie wszystkich
plików. Faza 0 ustala ostateczny zakres po inwentarzu.

## 16. Ryzyka i decyzje kontrolne

| Ryzyko | Zabezpieczenie |
|---|---|
| Przypadkowa zmiana starego brzmienia | osobny LegacyFixedV1 i bitowe goldeny |
| Utrata bitów seeda w JS | bigint oraz high/low u32; brak number |
| Zmiana obrazu stereo | dsp_correlation_v1 i testy strukturalne/IACC |
| Zależność od kolejności obiektów | adresowanie po trwałym object_slot |
| Reseed tylko części stanu | niezmienna polityka i testy przebudowy |
| Koszt w callbacku | SeedMaterialV1 dla MAX_SOURCES liczony tylko w konstruktorze NoiseEngine |
| Istniejące alokacje podczas przebudowy | pomiar legacy oraz zero dodatkowych alokacji/bajtów w seeded |
| Różne bloki zmieniają automatykę | kanoniczny, zapisany RenderScheduleV1 |
| Seed zmienia przestrzeń | spread oraz losowa topologia pogłosu pozostają stałe w V1 |
| Fałszywa obietnica WASM | jawnie ograniczony wrapper offline |
| Zmiana RNG przez aktualizację zależności | dokładny pin i known-answer test |
| Mieszanie akustyki pokoju z realizacją szumu | pogłosowa topologia poza root_seed V1 |
| Optymalizator wybiera „szczęśliwy” seed | seed nie jest parametrem presetu |

## 17. Sposób włączenia do P-06 NMM

Po spełnieniu DoD DSP:

1. NMM zachowuje własne 32-bajtowe drzewo dla całego eksperymentu.
2. Dla adresu evaluation/panel/replicate/audio bierze dokładnie zdefiniowany
   liść u64 little-endian.
3. Ten u64 przekazuje do NoiseEngine::seeded lub
   NoiseEngine::seeded_with_source_count.
4. Raport NMM zapisuje:
   - run_seed NMM;
   - panel i replicate_index;
   - wynikowy dsp_root_seed;
   - commit DSP;
   - effective renderer revision odczytaną z instancji;
   - base renderer revision, jeżeli jest raportowana osobno;
   - seed derivation revision;
   - correlation policy revision;
   - render schedule revision i pełną listę bloków, jeżeli przebieg zawiera
     automatyzację.
5. NMM nie zna i nie odtwarza domen wewnętrznych DSP.

To tworzy wyraźną granicę odpowiedzialności: NMM wybiera realizację audio, a
DSP gwarantuje, co ta realizacja oznacza.

## 18. Podstawa naukowa i techniczna

### Rozdzielanie i reprodukcja strumieni

- NumPy, Parallel random number generation:
  https://numpy.org/doc/stable/reference/random/parallel.html
- Salmon i in., Parallel Random Numbers: As Easy as 1, 2, 3:
  https://random123.com/
- BLAKE3 derive_key, dokumentacja API Rust:
  https://docs.rs/blake3/latest/blake3/fn.derive_key.html
- fastrand 2.3.0, dokumentacja Rng:
  https://docs.rs/fastrand/2.3.0/fastrand/struct.Rng.html
- Monks i in., Strengthening the Reporting of Empirical Simulation Studies:
  https://doi.org/10.1080/17477778.2018.1442155

Wniosek: strumienie mają trwałe adresy i jawne wersje; raport zapisuje dane
potrzebne do powtórzenia wyniku.

### Neuroakustyka i korelacja międzyuszna

- Bernstein i Trahiotis, binaural detection with interaural correlation:
  https://pmc.ncbi.nlm.nih.gov/articles/PMC2668169/
- Palmer, Jiang i McAlpine, neural sensitivity to interaural timing:
  https://doi.org/10.1152/jn.1999.81.2.722
- Bernstein i Trahiotis, discrimination of interaural correlation:
  https://pubmed.ncbi.nlm.nih.gov/1737879/

Wniosek: korelacja między uszami jest słyszalną właściwością sygnału. Dlatego
zmiana seedów nie może zmienić obecnej topologii mono/HRTF zwykłego obiektu ani
niezależnej konstrukcji anchoru.

### API Swift i WASM

- UniFFI, interfaces and constructors:
  https://mozilla.github.io/uniffi-rs/0.31/types/interfaces.html
- UniFFI, Swift bindings overview:
  https://mozilla.github.io/uniffi-rs/next/swift/overview.html
- wasm-bindgen, mapowanie typów liczbowych:
  https://wasm-bindgen.github.io/wasm-bindgen/reference/types/numbers.html
- wasm-bindgen-test:
  https://wasm-bindgen.github.io/wasm-bindgen/wasm-bindgen-test/index.html

Wniosek: Swift może zachować UInt64, natomiast JavaScript musi użyć bigint albo
dwóch części u32. Automatyczna konwersja liczb wasm-bindgen nie waliduje
zakresu, dlatego publiczna warstwa sprawdza wartość przed castem. W obu
przypadkach finalny seed DSP jest dokładnie tym samym 64-bitowym numerem.

## 19. Warunek rozpoczęcia P-06

Warunek został spełniony 2026-09-17. Repozytorium DSP dostarczyło:

- commit lub tag z pełnym DoD;
- finalne nazwy API Rust, Swift i WASM;
- manifest seedów i goldenów;
- potwierdzenie zgodności legacy;
- wyniki testów realtime, CPU i neuroakustycznych.

P-06 może korzystać z wydanego API DSP. Jego status pozostaje `planned`, dopóki
nie zostaną wdrożone i odebrane osobne zmiany seedów wewnątrz NMM.
