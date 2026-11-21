# nc-audio/src

| Dosya | İçerik |
|---|---|
| `lib.rs` | Genel API, global `Engine`, `pump` döngüsü, adaptif tampon derinliği |
| `mixer.rs` | `Mixer`: 12 ses havuzu, öncelik/çalma, ADPCM yeniden örnekleme, ana ses, yumuşak sınırlayıcı |
| `synth.rs` | Tam sayı sinüs (`sine_q15`), tınılar, zarf, `SynthVoice` |
| `adpcm.rs` | `NCSN` bloğunu çözen IMA-ADPCM okuyucusu |
| `theme.rs` | `Sound` listesi, nota dizileri, öncelikler |
| `tests.rs` | Ana makinede çalışan birim testleri |
