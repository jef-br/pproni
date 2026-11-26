# 3 · EIX, the edge verdict `BUILT`

[Up: table of contents](TOC-specs.md)

**EIX** (edge intersection) is the Shot Classifier's answer to one question: which of the
picture's four edges does the subject run off? Routing keys on it ([§5](05-routing.md)).

## What it says

Four characters, one per edge, always in the order top, right, bottom, left (TRBL):

* `1`: the subject runs off this edge.
* `0`: it does not.

Examples: `1000` top only, `0110` right and bottom, `0101` left and right, `1111` all four,
`0000` none.

EIX says nothing about whether a subject was found. `0000` only means that no edge was found to be
intersected. An empty mask gives `0000`: all four edge checks ran and found nothing. Whether the
mask is a usable subject is a separate signal, `mask_too_small` (see *What EIX is not*).

## No verdict: `____`

When EIX cannot be set at all, it is written as four underscores, `____`: the same width as a real
verdict, read as "EIX is undefined". Today this happens only after a model error (an `Err` from
the model run on that image): there is no mask, so there is nothing to check. That error is hidden
today and the image goes to R3; fixing it is #31.

## How it is decided

Per edge, in order ([§3.1](03-shot-classifier.md) has the detail):

1. **Gate.** Is any mask pixel within `GATE_MARGIN_PX` of this edge? No: `0`.
2. **Refine.** At full resolution, does the refined mask reach the edge? No: `0`.
3. **Graze test.** Does the subject run *off* the edge, or only *along* it? Along: `0`.

Only an edge that passes all three gets `1`.

EIX is decided again, with the same checks and no second model run, when the picture is cut after
the model: after a headcut and after a frame is peeled. The last EIX is the one routing uses.

## What EIX is not

* **Not a box test.** A box can touch an edge while the subject's outline does not, so EIX comes
  from the mask, not from a box ([§3](03-shot-classifier.md)).
* **Not changed by `mask_too_small`.** That flag is a separate signal: the router reads it and sends
  the image to R2. EIX always shows the edges the image was really judged and routed on; a tag must
  never claim an edge the pipeline did not route on.

## Where it shows

* In the output file name, only with the `PPRONI_DEBUG_TAGS` marker:
  `<stem>--EIX=<trbl>[--BGC=<hex>][--FGC=<hex>].jpg`. The `=` stands in for `:`, which Windows
  file names do not allow. The rest of that name: [architecture: debug tags](../architecture/02-debug-tags.md).
* In the trace log (`<stem>  -  log.txt`), with the same marker.
* In the `run_dir` dev harness: its console and `zzz_results.json`.

In the code, `shot_classifier::shotcode::eix_bits` writes the four characters from
`touches_edges`, and `NO_VERDICT_EIX` is `____`.
