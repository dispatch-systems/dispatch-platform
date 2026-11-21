import { useEffect, useRef, useState, type ChangeEvent } from 'react';
import { Camera, ImagePlus, RotateCw } from 'lucide-react';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';

export type PickedPhoto = { file: File; turns: number };

/**
 * The photo of a signed paper: chosen from the device's files, which a phone or tablet offers
 * its camera beside, or on a phone taken with the camera at once.
 */
export function PhotoPicker({
  photo,
  change,
  disabled,
}: {
  photo?: PickedPhoto;
  change: (photo: PickedPhoto | undefined) => void;
  disabled?: boolean;
}) {
  /** The input a photo was last picked through, which Retake opens again. */
  const input = useRef<HTMLInputElement | null>(null);
  const pick = (event: ChangeEvent<HTMLInputElement>) => {
    input.current = event.target;
    const file = event.target.files?.[0];
    if (file) change({ file, turns: 0 });
  };
  const [preview, setPreview] = useState<{ file: File; src: string }>();
  const [error, setError] = useState('');
  const file = photo?.file;
  useEffect(() => {
    setError('');
    if (!file) {
      setPreview(undefined);
      return;
    }
    const src = URL.createObjectURL(file);
    setPreview({ file, src });
    return () => URL.revokeObjectURL(src);
  }, [file]);
  return (
    <div className="conduct-field">
      <div className="conduct-photo-picks">
        <label className="conduct-photo">
          <span className="conduct-label">Signed copy</span>
          <span className="conduct-photo-pick">
            <ImagePlus size={20} />
            {photo ? (
              photo.file.name
            ) : (
              <>
                <span className="conduct-photo-wide">Take a photo or choose a file</span>
                <span className="conduct-photo-phone">Choose photo</span>
              </>
            )}
          </span>
          <input ref={input} type="file" accept="image/*" disabled={disabled} onChange={pick} />
        </label>
        <label className="conduct-photo conduct-photo-take">
          <span className="conduct-photo-pick">
            <Camera size={20} />
            Take photo
          </span>
          <input
            type="file"
            accept="image/*"
            capture="environment"
            disabled={disabled}
            onChange={pick}
          />
        </label>
      </div>
      {photo && preview?.file === photo.file && (
        <>
          <div className="conduct-photo-preview">
            <img
              src={preview.src}
              alt="Selected signed copy"
              style={{ transform: `rotate(${photo.turns * 90}deg)` }}
              onError={() => setError('Choose a photo the browser can open.')}
            />
          </div>
          <div className="heading-actions">
            <button
              type="button"
              disabled={disabled}
              onClick={() => change({ ...photo, turns: (photo.turns + 1) % 4 })}
            >
              <RotateCw size={16} />
              Rotate
            </button>
            <button type="button" disabled={disabled} onClick={() => input.current?.click()}>
              Retake
            </button>
          </div>
        </>
      )}
      <ErrorBox message={error} />
    </div>
  );
}
