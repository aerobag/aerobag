export type ImageViewportState = {
  left: number;
  top: number;
  zoom: number;
};

export function clampImageZoom(zoom: number) {
  return Math.max(1, Math.min(zoom, 8));
}

export function createInitialImageViewport(
  imageWidth: number,
  imageHeight: number,
  viewportWidth: number,
  viewportHeight: number,
): ImageViewportState {
  const fitScale = Math.min(viewportWidth / imageWidth, viewportHeight / imageHeight);
  const width = imageWidth * fitScale;
  const height = imageHeight * fitScale;
  return {
    left: (viewportWidth - width) / 2,
    top: (viewportHeight - height) / 2,
    zoom: 1,
  };
}

export function imageDisplaySize(
  imageWidth: number,
  imageHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  zoom: number,
) {
  const fitScale = Math.min(viewportWidth / imageWidth, viewportHeight / imageHeight);
  const scale = fitScale * zoom;
  return {
    width: imageWidth * scale,
    height: imageHeight * scale,
  };
}

export function clampImageViewport(
  state: ImageViewportState,
  imageWidth: number,
  imageHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  overscrollPx: number,
): ImageViewportState {
  const size = imageDisplaySize(imageWidth, imageHeight, viewportWidth, viewportHeight, state.zoom);
  const minLeft = viewportWidth - overscrollPx - size.width;
  const maxLeft = overscrollPx;
  const minTop = viewportHeight - overscrollPx - size.height;
  const maxTop = overscrollPx;
  return {
    left: clampToRange(state.left, minLeft, maxLeft),
    top: clampToRange(state.top, minTop, maxTop),
    zoom: clampImageZoom(state.zoom),
  };
}

export function dragImageViewport(
  state: ImageViewportState,
  dx: number,
  dy: number,
  imageWidth: number,
  imageHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  overscrollPx: number,
) {
  return clampImageViewport(
    {
      ...state,
      left: state.left + dx,
      top: state.top + dy,
    },
    imageWidth,
    imageHeight,
    viewportWidth,
    viewportHeight,
    overscrollPx,
  );
}

export function zoomImageAroundPoint(
  state: ImageViewportState,
  anchorX: number,
  anchorY: number,
  nextZoom: number,
  imageWidth: number,
  imageHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  overscrollPx: number,
) {
  const zoom = clampImageZoom(nextZoom);
  const scale = zoom / state.zoom;
  return clampImageViewport(
    {
      left: anchorX - (anchorX - state.left) * scale,
      top: anchorY - (anchorY - state.top) * scale,
      zoom,
    },
    imageWidth,
    imageHeight,
    viewportWidth,
    viewportHeight,
    overscrollPx,
  );
}

function clampToRange(value: number, min: number, max: number) {
  if (min > max) {
    return (min + max) / 2;
  }
  return Math.min(max, Math.max(min, value));
}
