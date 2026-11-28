"""One-shot entry point for Faber's checked audio environments; all models are local."""
import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
os.environ['HF_HUB_OFFLINE'] = '1'
os.environ['TRANSFORMERS_OFFLINE'] = '1'
os.environ['HF_HUB_DISABLE_TELEMETRY'] = '1'
os.environ['DO_NOT_TRACK'] = '1'
os.environ['PYTHONDONTWRITEBYTECODE'] = '1'
sys.dont_write_bytecode = True
CACHE = tempfile.TemporaryDirectory(prefix='faber-audio-')
os.environ['NUMBA_CACHE_DIR'] = CACHE.name


def main():
    name = next(line[6:] for line in (ROOT / 'instructions/SKILL.md').read_text().splitlines() if line.startswith('name: '))
    parser = argparse.ArgumentParser(description=f'Installed {name}; quality scores are advisory until calibrated.')
    parser.add_argument('input', nargs='?', help='Generated audio file, or a narration paragraph (put -- before text starting with -)')
    parser.add_argument('--text-file', type=Path, help='UTF-8 narration text file, instead of the input argument')
    parser.add_argument('--output', help='Output WAV for speech generation')
    parser.add_argument('--language', default='en', help='Language code for transcription or narration')
    args = parser.parse_args()
    if args.text_file is not None:
        if name != 'chatterbox-multilingual' or args.input is not None:
            parser.error('--text-file is only for narration, instead of the input argument')
        try:
            args.input = args.text_file.read_text(encoding='utf-8')
        except (OSError, UnicodeError) as error:
            parser.error(f'Cannot read the narration text file: {error}')
    if args.input is None or not args.input.strip():
        parser.error('An input argument or narration --text-file is required')
    if name == 'chatterbox-multilingual':
        if not args.output:
            parser.error('--output is required for narration')
        output = Path(args.output)
        if output.suffix.lower() != '.wav':
            parser.error('--output must have a .wav extension')
        if not output.parent.is_dir() or (output.exists() and not output.is_file()):
            parser.error('--output needs an existing directory and a file name')
        try:
            with tempfile.TemporaryFile(dir=output.parent):
                pass
            if output.exists() and not os.access(output, os.W_OK):
                parser.error('--output is not writable')
        except OSError as error:
            parser.error(f'The output directory is not writable: {error}')
    sys.path.insert(0, str(ROOT / 'payload'))
    if name == 'utmosv2':
        sys.path.insert(0, str(ROOT / 'payload/UTMOSv2-cc2700db57bb83ee13dc31ebe1b868c254e15d09'))
    if name == 'chatterbox-multilingual':
        sys.path.insert(0, str(ROOT / 'library/antlr4/antlr4-python3-runtime-4.9.3/src'))
    if name == 'faster-whisper':
        from faster_whisper import WhisperModel
        model = WhisperModel(str(ROOT / 'weights'), device='cpu', compute_type='int8', local_files_only=True)
        segments, info = model.transcribe(args.input, language=args.language, word_timestamps=True)
        result = {'language': info.language, 'segments': [asdict(segment) for segment in segments]}
    elif name == 'speechmos':
        import librosa
        from speechmos import dnsmos
        samples, _ = librosa.load(args.input, sr=16000, mono=True)
        result = {'advisory': True, 'dnsmos': dnsmos.run(samples, sr=16000)}
    elif name == 'audiobox-aesthetics':
        from audiobox_aesthetics.infer import initialize_predictor
        predictor = initialize_predictor(ckpt=str(ROOT / 'weights/checkpoint.pt'))
        result = {'advisory': True, 'attribution': 'Audiobox Aesthetics, Meta, CC-BY-4.0; https://huggingface.co/facebook/audiobox-aesthetics', 'scores': predictor.forward([{'path': args.input}])}
    elif name == 'utmosv2':
        from transformers import AutoModel, AutoFeatureExtractor
        # Keep the architecture's model identity while routing its two pretrained loads to checked local files.
        original_model = AutoModel.from_pretrained
        original_features = AutoFeatureExtractor.from_pretrained
        AutoModel.from_pretrained = lambda *a, **kw: original_model(str(ROOT / 'weights/wav2vec2'), local_files_only=True)
        AutoFeatureExtractor.from_pretrained = lambda *a, **kw: original_features(str(ROOT / 'weights/wav2vec2'), local_files_only=True)
        import utmosv2
        import utmosv2.config.fusion_stage3 as config
        config.model.multi_spec.pretrained = False
        model = utmosv2.create_model(checkpoint_path=str(ROOT / 'weights/fold0_s42_best_model.pth'), device='cuda')
        result = {'advisory': True, 'utmosv2': model.predict(input_path=args.input, device='cuda', num_workers=0, batch_size=1)}
    elif name == 'chatterbox-multilingual':
        import soundfile
        import huggingface_hub
        import spacy_pkuseg
        segmenter = spacy_pkuseg.pkuseg
        spacy_pkuseg.pkuseg = lambda *a, **kw: segmenter(model_name=str(ROOT / 'library/spacy_ontonotes'), user_dict='default', postag=False)
        # The upstream tokenizer requests this dictionary by name even when its model was loaded locally.
        def local_file(repo_id, filename, **kwargs):
            path = ROOT / 'weights' / filename
            if repo_id != 'ResembleAI/chatterbox' or not path.is_file():
                raise FileNotFoundError('Unpinned model file: ' + filename)
            return str(path)
        huggingface_hub.hf_hub_download = local_file
        from chatterbox.mtl_tts import ChatterboxMultilingualTTS
        model = ChatterboxMultilingualTTS.from_local(ROOT / 'weights', device='cuda')
        samples = model.generate(args.input, language_id=args.language)
        soundfile.write(args.output, samples.squeeze().cpu().numpy(), model.sr)
        result = {'output': str(Path(args.output).resolve()), 'sample_rate': model.sr}
    else:
        parser.error('Unknown installed runtime')
    print(json.dumps(result, ensure_ascii=False, default=lambda value: value.item() if hasattr(value, 'item') else value.tolist()))


if __name__ == '__main__':
    main()
