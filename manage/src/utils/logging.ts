export enum Level {
  Error,
  Warn,
  Info,
  Debug,
  Trace,
}

let level: Level = Level.Info;

export function setLevel(lvl: Level) {
  level = lvl;
}

export function log(lvl: Level, msg: string, ...args: unknown[]) {
  if (lvl > level) return;

  let color = "#fff";

  switch (lvl) {
    case Level.Error:
      color = "#fb726a";
      break;
    case Level.Warn:
      color = "#ff9724";
      break;
    case Level.Info:
      color = "#54f351";
      break;
    case Level.Debug:
      color = "#02eaed";
      break;
    case Level.Trace:
      color = "#c484ff";
      break;
  }

  const style =
    `background: ${color}; color: #111; padding: 0.1rem 0.25rem; border-radius: 0.25rem;`;

  switch (lvl) {
    case Level.Error:
      console.error(`%cERROR%c ${msg}`, style, "", ...args);
      break;
    case Level.Warn:
      console.warn(`%cWARN%c ${msg}`, style, "", ...args);
      break;
    case Level.Info:
      console.info(`%cINFO%c ${msg}`, style, "", ...args);
      break;
    case Level.Debug:
      console.debug(`%cDEBUG%c ${msg}`, style, "", ...args);
      break;
    case Level.Trace:
      console.debug(`%cTRACE%c ${msg}`, style, "", ...args);
      break;
    default:
      console.log(msg, ...args);
      break;
  }
}

export function error(msg: string, ...args: unknown[]) {
  log(Level.Error, msg, ...args);
}

export function warn(msg: string, ...args: unknown[]) {
  log(Level.Warn, msg, ...args);
}

export function info(msg: string, ...args: unknown[]) {
  log(Level.Info, msg, ...args);
}

export function debug(msg: string, ...args: unknown[]) {
  log(Level.Debug, msg, ...args);
}

export function trace(msg: string, ...args: unknown[]) {
  log(Level.Trace, msg, ...args);
}
