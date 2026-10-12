import { z } from 'zod/mini';
import { employeeSyncSchema } from './employee-sync-runtime.js';
import type {
  Employee,
  EmployeeTimecardResponse,
  EmployeesResponse,
  DailyTimecards,
  Punch,
  PaycomDay,
  MealComparison,
  MealPeriod,
  MealPeriodGaps,
  MealEmployee,
  MealAssessment,
  PaycomPreferences,
  PaycomSettings,
} from './index.js';
import { jobSchema, jobsSchema } from '../../../core/collection/api/runtime.js';
import { count, text, type Replies } from '../../../core/foundation/api/runtime.js';
import {
  employeesPath,
  mealBreaksPath,
  mealPeriodGapsPath,
  mealPeriodPath,
  paycomSettingsPath,
} from './urls.js';

const optionalText = z.optional(z.nullable(text));
const whole = z.number().check(z.int());
export const employeeSchema = z.object({
  code: text,
  name: text,
  department: text,
  position: text,
  station: text,
  active: z.boolean(),
}) satisfies z.ZodMiniType<Employee>;
const punchSchema = z.object({
  inKind: z.optional(z.nullable(z.enum(['IN DAY', 'IN LUNCH']))),
  outKind: z.optional(z.nullable(z.enum(['OUT LUNCH', 'OUT DAY']))),
  in: z.nullable(text),
  out: z.nullable(text),
  hours: z.nullable(z.number()),
}) satisfies z.ZodMiniType<Punch>;
const clock = z.object({ minute: whole, day: whole });
const lunch = z.object({ out: z.nullable(clock), in: z.nullable(clock) });
const day = z.object({
  inDay: z.nullable(clock),
  outDay: z.nullable(clock),
  lunches: z.array(lunch),
  events: z.array(z.object({ kind: text, time: z.nullable(clock), raw: text })),
  review: z.boolean(),
  legacy: z.boolean(),
}) satisfies z.ZodMiniType<PaycomDay>;
const card = z.object({
  employeeCode: text,
  date: text,
  hours: z.number(),
  status: text,
  punches: z.array(punchSchema),
  sourceUrl: optionalText,
});
const period = z.object({ from: text, to: text });
export const employeeTimecardSchema = z.object({
  employee: employeeSchema,
  timecards: z.array(
    z.extend(card, {
      assessment: day,
      regularMinutes: z.nullable(count),
      overtimeMinutes: z.nullable(count),
    }),
  ),
  period,
  previousPeriod: z.nullable(period),
  nextPeriod: z.nullable(period),
  collectedAt: z.nullable(text),
  syncStatus: z.nullable(
    z.enum(['queued', 'running', 'waiting_verification', 'succeeded', 'failed', 'cancelled']),
  ),
}) satisfies z.ZodMiniType<EmployeeTimecardResponse>;
export const employeesSchema = z.object({
  employees: z.array(employeeSchema),
  total: count,
  collectedAt: z.nullable(text),
}) satisfies z.ZodMiniType<EmployeesResponse>;
export const dailyTimecardsSchema = z.object({
  rows: z.array(z.extend(card, { name: text })),
  collectedAt: z.nullable(text),
  available: z.boolean(),
}) satisfies z.ZodMiniType<DailyTimecards>;
const cortexMeal = z.object({
  mealId: text,
  itineraryId: text,
  cortexId: text,
  driverName: text,
  station: text,
  timezone: text,
  collectedAt: text,
  lastDelivery: z.nullable(text),
  start: text,
  end: z.nullable(text),
  firstDelivery: z.nullable(text),
  beforeStatus: text,
  afterStatus: text,
  sourceUrl: optionalText,
  lastDeliveryUrl: optionalText,
  firstDeliveryUrl: optionalText,
});
const gap = z.object({ milliseconds: count, overLimit: z.boolean() });
const assessment = z.object({
  paycom: day,
  pairs: z
    .array(
      z.object({
        cortexIndex: z.nullable(count),
        lunchIndex: z.nullable(count),
        out: z.nullable(clock),
        into: z.nullable(clock),
        outDifference: z.nullable(whole),
        inDifference: z.nullable(whole),
        gaps: z.object({ before: z.nullable(gap), after: z.nullable(gap) }),
      }),
    )
    .check(z.minLength(1)),
  different: z.boolean(),
  missing: z.boolean(),
  status: z.enum([
    'flex_only',
    'no_flex_meal',
    'review_punches',
    'missing_lunch',
    'review_pairing',
    'missing_data',
    'different',
    'same',
    'cortex_unreadable',
  ]),
  longGap: z.boolean(),
  lateIn: z.boolean(),
}) satisfies z.ZodMiniType<MealAssessment>;
const mealEmployee = z
  .object({
    id: text,
    name: text,
    paycom: z.nullable(
      z.object({
        employeeCode: text,
        name: text,
        department: optionalText,
        station: optionalText,
        date: optionalText,
        hours: z.optional(z.nullable(z.number())),
        status: text,
        punches: z.array(punchSchema),
        sourceUrl: optionalText,
      }),
    ),
    cortex: z.array(cortexMeal),
    cortexUnreadable: z.array(
      z.object({
        itineraryId: text,
        cortexId: text,
        driverName: text,
        station: text,
        timezone: text,
        collectedAt: text,
        rule: text,
        sourceUrl: optionalText,
      }),
    ),
    assessment,
  })
  .check(
    z.superRefine((row, context) => {
      for (const pair of row.assessment.pairs) {
        if (
          (pair.cortexIndex !== null && pair.cortexIndex >= row.cortex.length) ||
          (pair.lunchIndex !== null && pair.lunchIndex >= row.assessment.paycom.lunches.length)
        )
          context.addIssue({ code: 'custom', message: 'Invalid assessment reference' });
      }
    }),
  ) satisfies z.ZodMiniType<MealEmployee>;
export const mealComparisonSchema = z.object({
  date: text,
  timezone: text,
  rows: z.array(mealEmployee),
  paycomCollectedAt: z.nullable(text),
  cortexPublications: z.array(
    z.object({
      station: text,
      timezone: text,
      collectedAt: text,
      id: optionalText,
      serviceAreaId: optionalText,
      provider: optionalText,
    }),
  ),
  drivers: z.array(
    z.object({
      id: text,
      name: text,
      paycomCode: z.nullable(text),
      matchType: z.enum(['name', 'saved', 'separate', 'unmatched']),
    }),
  ),
}) satisfies z.ZodMiniType<MealComparison>;
export const mealPeriodSchema = z.object({
  period,
  days: z.array(mealComparisonSchema),
  gaps: z.array(
    z.object({ date: text, meal: count, side: z.enum(['before', 'after']), milliseconds: count }),
  ),
  excessMilliseconds: count,
}) satisfies z.ZodMiniType<MealPeriod>;
export const mealPeriodGapsSchema = z.object({
  period: z.nullable(period),
  counts: z.record(text, count),
}) satisfies z.ZodMiniType<MealPeriodGaps>;
const preferences = z.object({
  opening_page: z.enum(['timecards', 'meal-breaks', 'employees']),
  rows_per_page: z.union([z.literal(25), z.literal(50), z.literal(100)]),
  name_order: z.enum(['first_last', 'last_first']),
  default_sort: z.enum(['employeeName', 'condition', 'inDay']),
  department: z.nullable(text),
  station: z.nullable(text),
  columns: z.array(z.enum(['inDay', 'outLunch', 'inLunch', 'outDay', 'totalHours', 'condition'])),
  driver_departments: z.nullable(z.array(text)),
  late_da_time: text.check(z.regex(/^(?:[01]\d|2[0-3]):[0-5]\d$/)),
  late_da_departments: z.array(text),
}) satisfies z.ZodMiniType<PaycomPreferences>;

export const paycomSettingsSchema = z.object({
  revision: count,
  values: preferences,
  history: z.array(z.object({ revision: count, at: text, values: preferences })),
  options: z.object({
    departments: z.array(z.object({ value: text, count })),
    stations: z.array(text),
  }),
}) satisfies z.ZodMiniType<PaycomSettings>;

/** The replies of Timecard's reads, its Paycom settings and its jobs and collections. */
export const replies: Replies = (route, method) => {
  if (route === '/api/dsp/jobs/employees') return employeeSyncSchema;
  if (method === 'GET') {
    if (route === '/api/dsp/jobs') return jobsSchema;
    if (route === paycomSettingsPath) return paycomSettingsSchema;
    if (route === employeesPath) return employeesSchema;
    if (/^\/api\/dsp\/employees\/[^/]+$/.test(route)) return employeeTimecardSchema;
    if (route === '/api/dsp/timecards') return dailyTimecardsSchema;
    if (route === mealBreaksPath) return mealComparisonSchema;
    if (route === mealPeriodPath) return mealPeriodSchema;
    if (route === mealPeriodGapsPath) return mealPeriodGapsSchema;
    return undefined;
  }
  if (route === '/api/dsp/jobs') return jobSchema;
  if (route === paycomSettingsPath) return paycomSettingsSchema;
  if (route === '/api/dsp/cortex/meal-breaks/collect') return jobSchema;
  if (route === '/api/dsp/jobs/meal-breaks') return z.object({ date: text, jobs: jobsSchema });
  return undefined;
};
