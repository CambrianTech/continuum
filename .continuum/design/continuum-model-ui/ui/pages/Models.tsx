
// ui/pages/Models.tsx
import React, { useEffect, useState } from 'react';
import { ModelCard } from '../components/ModelCard';

export default function ModelsPage() {
  const [models, setModels] = useState([]);

  useEffect(() => {
    fetch('/registry.json')
      .then((res) => res.json())
      .then((data) => setModels(data.models));
  }, []);

  const handleInstall = async (id: string) => {
    const agree = confirm("Do you accept the agent agreement?");
    if (!agree) return;
    const res = await fetch(`/api/install/${id}`, { method: 'POST' });
    const data = await res.json();
    alert(data.message || 'Done!');
  };

  return (
    <div className="p-6">
      <h1 className="text-2xl font-bold mb-4">🧠 Available Models</h1>
      <div className="flex flex-wrap gap-4">
        {models.map((m: any) => (
          <ModelCard key={m.id} model={m} onInstall={handleInstall} />
        ))}
      </div>
    </div>
  );
}
