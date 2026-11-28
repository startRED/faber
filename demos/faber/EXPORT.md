# Faber product demo preview

Serve this folder over HTTP and open `faber.html`. JavaScript modules require an HTTP origin; opening the HTML by
double-clicking it is not supported. The iframe, assets and `notices/` folder must stay beside that page.

This is an illustrated conversation, not a running AI or a working planner. It starts no AI process, spends no quota
and contains only synthetic content. Playback controls include pause/play, replay, next frame and 0.25x slow motion.
Reduced motion shows the completed result. The preview has a separate composition for a 390 px column.

The original sources live in `demos/faber/` in the Faber repository. From that checkout, `npm run demo -- --lang pt-BR`
prepares and serves the illustration with its verified upstream runtime; `npm run demo -- --build --lang pt-BR` exports
this standalone folder. Supported languages are English, Brazilian Portuguese, Spanish, French and German.

Runtime and font licenses are retained in `notices/`. Stageshow is downloaded from its original source at Faber's
catalog pin; its source and dependencies are not included in the Faber application.
