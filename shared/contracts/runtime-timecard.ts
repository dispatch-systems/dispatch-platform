import { z } from 'zod';
import type {
  Employee,
  EmployeeTimecardResponse,
  EmployeesResponse,
  DailyTimecards,
  Punch,
  PaycomDay,
  MealComparison,
  MealEmployee,
  MealAssessment,
  PaycomPreferences,
  PaycomSettings,
} from './timecard.js';
import { jobSchema, jobsSchema } from './runtime-collection.js';
import { count, text, type Replies } from './runtime.js';

const optionalText = text.nullable().optional();
export const employeeSchema = z.object({
  code: text,
  name: text,
  department: text,
  position: text,
  station: text,
  active: z.boolean(),
}) satisfies z.ZodType<Employee>;
const punchSchema = z.object({
  inKind: z.enum(['IN DAY', 'IN LUNCH']).nullable().optional(),
  outKind: z.enum(['OUT LUNCH', 'OUT DAY']).nullable().optional(),
  in: text.nullable(),
  out: text.nullable(),
  hours: z.number().nullable(),
}) satisfies z.ZodType<Punch>;
const clock = z.object({ minute: z.number().int(), day: z.number().int() });
const lunch = z.object({ out: clock.nullable(), in: clock.nullable() });
const day = z.object({
  inDay: clock.nullable(),
  outDay: clock.nullable(),
  lunches: z.array(lunch),
  events: z.array(z.object({ kind: text, time: clock.nullable(), raw: text })),
  review: z.boolean(),
  legacy: z.boolean(),
}) satisfies z.ZodType<PaycomDay>;
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
  timecards: z.array(card.extend({ assessment: day })),
  period,
  previousPeriod: period.nullable(),
  nextPeriod: period.nullable(),
  collectedAt: text.nullable(),
  syncStatus: z
    .enum(['queued', 'running', 'waiting_verification', 'succeeded', 'failed', 'cancelled'])
    .nullable(),
}) satisfies z.ZodType<EmployeeTimecardResponse>;
export const employeesSchema = z.object({
  employees: z.array(employeeSchema),
  total: count,
  collectedAt: text.nullable(),
}) satisfies z.ZodType<EmployeesResponse>;
export const dailyTimecardsSchema = z.object({
  rows: z.array(card.extend({ name: text })),
  collectedAt: text.nullable(),
  available: z.boolean(),
}) satisfies z.ZodType<DailyTimecards>;
const cortexMeal = z.object({
  mealId: text,
  itineraryId: text,
  cortexId: text,
  driverName: text,
  station: text,
  timezone: text,
  collectedAt: text,
  lastDelivery: text.nullable(),
  start: text,
  end: text.nullable(),
  firstDelivery: text.nullable(),
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
        cortexIndex: count.nullable(),
        lunchIndex: count.nullable(),
        out: clock.nullable(),
        into: clock.nullable(),
        outDifference: z.number().int().nullable(),
        inDifference: z.number().int().nullable(),
        gaps: z.object({ before: gap.nullable(), after: gap.nullable() }),
      }),
    )
    .min(1),
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
  ]),
  longGap: z.boolean(),
  lateIn: z.boolean(),
}) satisfies z.ZodType<MealAssessment>;
const mealEmployee = z
  .object({
    id: text,
    name: text,
    paycom: z
      .object({
        employeeCode: text,
        name: text,
        department: optionalText,
        station: optionalText,
        date: optionalText,
        hours: z.number().nullable().optional(),
        status: text,
        punches: z.array(punchSchema),
        sourceUrl: optionalText,
      })
      .nullable(),
    cortex: z.array(cortexMeal),
    assessment,
  })
  .superRefine((row, context) => {
    for (const pair of row.assessment.pairs) {
      if (
        (pair.cortexIndex !== null && pair.cortexIndex >= row.cortex.length) ||
        (pair.lunchIndex !== null && pair.lunchIndex >= row.assessment.paycom.lunches.length)
      )
        context.addIssue({ code: 'custom', message: 'Invalid assessment reference' });
    }
  }) satisfies z.ZodType<MealEmployee>;
export const mealComparisonSchema = z.object({
  date: text,
  timezone: text,
  rows: z.array(mealEmployee),
  paycomCollectedAt: text.nullable(),
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
      paycomCode: text.nullable(),
      matchType: z.enum(['name', 'saved', 'separate', 'unmatched']),
    }),
  ),
}) satisfies z.ZodType<MealComparison>;
const preferences = z.object({
  opening_page: z.enum(['timecards', 'meal-breaks', 'employees']),
  rows_per_page: z.union([z.literal(25), z.literal(50), z.literal(100)]),
  name_order: z.enum(['first_last', 'last_first']),
  default_sort: z.enum(['employeeName', 'condition', 'inDay']),
  department: text.nullable(),
  station: text.nullable(),
  columns: z.array(z.enum(['inDay', 'outLunch', 'inLunch', 'outDay', 'totalHours', 'condition'])),
  driver_departments: z.array(text).nullable(),
  late_da_time: text.regex(/^(?:[01]\d|2[0-3]):[0-5]\d$/),
  late_da_departments: z.array(text),
}) satisfies z.ZodType<PaycomPreferences>;

export const paycomSettingsSchema = z.object({
  revision: count,
  values: preferences,
  history: z.array(z.object({ revision: count, at: text, values: preferences })),
  options: z.object({
    departments: z.array(z.object({ value: text, count })),
    stations: z.array(text),
  }),
}) satisfies z.ZodType<PaycomSettings>;

/** The replies of Timecard's reads, its Paycom settings and its meal break collections. */
export const replies: Replies = (route, method) => {
  if (method === 'GET') {
    if (route === '/api/dsp/paycom/settings') return paycomSettingsSchema;
    if (route === '/api/dsp/employees') return employeesSchema;
    if (/^\/api\/dsp\/employees\/[^/]+$/.test(route)) return employeeTimecardSchema;
    if (route === '/api/dsp/timecards') return dailyTimecardsSchema;
    if (route === '/api/dsp/paycom/meal-breaks') return mealComparisonSchema;
    return undefined;
  }
  if (route === '/api/dsp/paycom/settings') return paycomSettingsSchema;
  if (route === '/api/dsp/cortex/meal-breaks/collect') return jobSchema;
  if (route === '/api/dsp/jobs/meal-breaks') return z.object({ date: text, jobs: jobsSchema });
  return undefined;
};
