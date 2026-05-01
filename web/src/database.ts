import localforage from "localforage";

const store = localforage.createInstance({
  name: "hypercutter",
  storeName: "symfiles",
});

export async function getSymFromDB(key: string): Promise<string | null> {
  return store.getItem(key);
}

export async function saveSymToDB(key: string, data: string): Promise<void> {
  await store.setItem(key, data);
}
