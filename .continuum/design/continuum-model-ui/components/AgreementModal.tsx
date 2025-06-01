
// components/AgreementModal.tsx
import React from 'react';

export function AgreementModal({ visible, onAccept, onCancel }: any) {
  if (!visible) return null;
  return (
    <div className="fixed inset-0 bg-black bg-opacity-40 flex items-center justify-center z-50">
      <div className="bg-white p-6 rounded-lg shadow-lg w-[90%] max-w-md">
        <h2 className="text-xl font-semibold mb-2">User Agreement</h2>
        <p className="text-sm text-gray-700 mb-4">
          By installing a model, you agree that large files may be downloaded or API keys stored locally.
        </p>
        <div className="flex justify-end gap-2">
          <button onClick={onCancel} className="text-sm bg-gray-300 px-3 py-1 rounded">Cancel</button>
          <button onClick={onAccept} className="text-sm bg-blue-600 text-white px-3 py-1 rounded">Accept</button>
        </div>
      </div>
    </div>
  );
}
