import { useEffect, useRef, useState } from 'react';
import { ImagePlus, RotateCw } from 'lucide-react';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';

export type PickedPhoto = { file: File; turns: number };
export function PhotoPicker({
  photo,
  change,
  disabled,
}: {
  photo?: PickedPhoto;
  change: (photo: PickedPhoto | undefined) => void;
  disabled?: boolean;
}) {
  const input = useRef<HTMLInputElement>(null);
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
      <label className="conduct-photo">
        <span className="conduct-label">
          Signed copy <span className="muted">(optional)</span>
        </span>
        <span className="conduct-photo-pick">
          <ImagePlus size={20} />
          {photo ? photo.file.name : 'Take a photo or choose a file'}
        </span>
        <input
          ref={input}
          type="file"
          accept="image/*"
          capture="environment"
          disabled={disabled}
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) change({ file, turns: 0 });
          }}
        />
      </label>
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
            <button
              type="button"
              disabled={disabled}
              onClick={() => {
                change(undefined);
                if (input.current) input.current.value = '';
              }}
            >
              Remove
            </button>
          </div>
        </>
      )}
      <ErrorBox message={error} />
    </div>
  );
}
