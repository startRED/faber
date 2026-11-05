/** The composer and group files use the same browser file reader. */
export function readAs(file: File, how: 'text' | 'dataUrl'): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    const failed = () => reject({ key: 'error.attachmentRead', detail: file.name });
    reader.onerror = failed;
    reader.onabort = failed;
    try {
      if (how === 'text') reader.readAsText(file);
      else reader.readAsDataURL(file);
    } catch { failed(); }
  });
}
