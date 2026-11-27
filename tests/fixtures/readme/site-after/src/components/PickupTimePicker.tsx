const OPENS = 7;
const CLOSES = 15;

/** Every half hour the shop is still open today: nobody can pick a time that has passed. */
export function pickupTimes(now: Date): string[] {
  const times: string[] = [];
  for (let hour = OPENS; hour < CLOSES; hour++) {
    for (const minute of [0, 30]) {
      if (hour * 60 + minute <= now.getHours() * 60 + now.getMinutes()) continue;
      times.push(`${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}`);
    }
  }
  return times;
}

export function PickupTimePicker(props: { value: string; onChange: (time: string) => void }) {
  const times = pickupTimes(new Date());
  if (!times.length) return <p role="status">We are closed for today. Orders open again at 7 a.m.</p>;
  return (
    <label>
      Pickup time
      <select value={props.value} onChange={(event) => props.onChange(event.target.value)}>
        {times.map((time) => (
          <option key={time} value={time}>{time}</option>
        ))}
      </select>
    </label>
  );
}
