import { useEffect, useState } from 'react';

interface ColonySelectorProps {
  onColonySelect: (colonyId: string | null) => void;
  onColoniesLoaded?: (colonies: string[]) => void;
  hide?: boolean;
}

export function ColonySelector({ onColonySelect, onColoniesLoaded, hide }: ColonySelectorProps) {
  const [colonies, setColonies] = useState<string[]>([]);
  const [selectedColony, setSelectedColony] = useState<string>('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function fetchColonies() {
      try {
        setLoading(true);
        const response = await fetch('/api/colonies');
        if (!response.ok) {
          throw new Error(`Failed to fetch colonies: ${response.statusText}`);
        }
        const data = await response.json();
        const coloniesList = data.colonies || [];
        setColonies(coloniesList);
        setError(null);
        
        if (onColoniesLoaded) {
          onColoniesLoaded(coloniesList);
        }
        
        // Auto-select if there's only one colony
        if (coloniesList.length === 1) {
          setSelectedColony(coloniesList[0]);
          onColonySelect(coloniesList[0]);
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Unknown error');
        setColonies([]);
        if (onColoniesLoaded) {
          onColoniesLoaded([]);
        }
      } finally {
        setLoading(false);
      }
    }

    fetchColonies();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    const value = event.target.value;
    setSelectedColony(value);
    onColonySelect(value === '' ? null : value);
  };

  if (hide) {
    return null;
  }

  if (loading) {
    return (
      <div className="mb-3">
        <div className="spinner-border spinner-border-sm text-light me-2" role="status">
          <span className="visually-hidden">Loading...</span>
        </div>
        <span className="text-light">Loading colonies...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="alert alert-danger" role="alert">
        Error: {error}
      </div>
    );
  }

  return (
    <div className="mb-4">
      <label htmlFor="colony-select" className="form-label text-light">
        Select Colony:
      </label>
      <select
        id="colony-select"
        className="form-select form-select-lg bg-dark text-light border-secondary"
        value={selectedColony}
        onChange={handleChange}
        style={{ maxWidth: '300px' }}
      >
        <option value="">-- Select a colony --</option>
        {colonies.map((colony) => (
          <option key={colony} value={colony}>
            {colony}
          </option>
        ))}
      </select>
    </div>
  );
}

