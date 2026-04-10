const DB_NAME = "hypercutter_symdb";
const DB_VERSION = 1;
const STORE_NAME = "symfiles";

let db: IDBDatabase | null = null;

export async function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (db) {
      resolve(db);
      return;
    }
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      db = request.result;
      resolve(db);
    };
    request.onupgradeneeded = (event) => {
      const database = (event.target as IDBOpenDBRequest).result;
      if (!database.objectStoreNames.contains(STORE_NAME)) {
        database.createObjectStore(STORE_NAME, { keyPath: "key" });
      }
    };
  });
}

export async function getSymFromDB(key: string): Promise<string | null> {
  const database = await openDB();
  return new Promise((resolve, reject) => {
    const tx = database.transaction(STORE_NAME, "readonly");
    const store = tx.objectStore(STORE_NAME);
    const request = store.get(key);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result?.data ?? null);
  });
}

export async function saveSymToDB(key: string, data: string): Promise<void> {
  const database = await openDB();
  return new Promise((resolve, reject) => {
    const tx = database.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.put({ key, data });
    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve();
  });
}
