
// components/ModelCard.tsx
import React from 'react';

export function ModelCard({ model, onInstall }: { model: any, onInstall: (id: string) => void }) {
  return (
    <div className="border rounded-xl p-4 shadow-md bg-white flex flex-col gap-2 w-full max-w-sm">
      <h2 className="text-xl font-semibold">{model.name}</h2>
      <p className="text-sm text-gray-600">{model.description}</p>
      <p className="text-xs text-gray-500">Provider: {model.provider}</p>
      <p className="text-xs text-gray-500">Cost: {model.cost}</p>
      <p className="text-xs text-gray-500">Speed: {model.speed}</p>
      <button
        onClick={() => onInstall(model.id)}
        className="bg-blue-600 text-white py-1 px-3 rounded-md hover:bg-blue-700 text-sm mt-2"
      >
        Install / Link
      </button>
    </div>
  );
}
