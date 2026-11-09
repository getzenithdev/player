type VisibilityCallback = (visible: boolean) => void;

function createVisibilityObserver(rootMargin: string) {
  const callbacks = new Map<Element, VisibilityCallback>();
  let observer: IntersectionObserver | undefined;
  return (element: Element, callback: VisibilityCallback) => {
    observer ??= new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          callbacks.get(entry.target)?.(entry.isIntersecting);
        }
      },
      { rootMargin },
    );
    callbacks.set(element, callback);
    observer.observe(element);
    return () => {
      callbacks.delete(element);
      observer?.unobserve(element);
      if (!callbacks.size) {
        observer?.disconnect();
        observer = undefined;
      }
    };
  };
}

// Share observers across thumbnails instead of allocating one per library row.
export const observeNearbyArtwork = createVisibilityObserver("240px");
export const observeVisibleArtwork = createVisibilityObserver("0px");
