<script lang="ts">
  import { type Portrait } from '$lib/api/portrait';
  import { type Viewport, heatmapImage, visibleBins } from '$lib/portrait';

  let {
    pixels,
    portrait,
    index,
    viewport,
    description,
  }: {
    pixels: Uint8Array;
    portrait: Portrait;
    index: number;
    viewport: Viewport;
    /** What the picture shows, for readers who will not see it. */
    description: string;
  } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);

  $effect(() => {
    const target = canvas;
    const image = heatmapImage(pixels, portrait, index, visibleBins(portrait, viewport));
    if (!target || image.width === 0 || image.height === 0) {
      return;
    }
    /* Sized to the columns actually held and stretched by CSS: scaling here
       would invent resolution the capture never had. Zooming re-slices the
       same bins, which is why the panel states their width. */
    target.width = image.width;
    target.height = image.height;
    target
      .getContext('2d')
      ?.putImageData(new ImageData(image.data, image.width, image.height), 0, 0);
  });
</script>

<!-- The description rides on a caption rather than on the canvas: a canvas
     carries no role a reader can be given, and a figure does. -->
<figure class="m-0">
  <canvas
    bind:this={canvas}
    aria-hidden="true"
    class="h-32 w-full rounded-sm bg-ink-50 [image-rendering:pixelated]"
  ></canvas>
  <figcaption class="sr-only">{description}</figcaption>
</figure>
