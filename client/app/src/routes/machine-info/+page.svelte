<script lang="ts">
	import MechaCompute from '$lib/images/svgs/MechaCompute.svg';
	import Layout from '../../shared/layout.svelte';
	import Icons from '../../shared/Icons.svelte';
	import { invoke } from '@tauri-apps/api';
	import { check_ping_status, get_machine_info } from '$lib/services';
	import { onMount } from 'svelte';
	import { machineInfo } from '$lib/stores';
	import type { MachineDataType } from '../../interfaces';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';

	// commit :
	// fix: store value, close error prompt

	let storeData: MachineDataType = $machineInfo;
	machineInfo.subscribe((value) => {
		storeData = value;
	});

	let machine_id: string = '-';
	let machine_name: string = 'My Machine';
	let machine_icon: string = '';
	let is_active: boolean = false;
	let is_error: boolean = false;
	let is_loading: boolean = true;

	let error_message: string = '';

	const get_machine_data = async () => {
		try {
			await get_machine_info();
			let { id, name, icon } = storeData;
			machine_id = id;
			machine_name = name ? name : 'My Machine';
			machine_icon = icon || '';
		} catch (error) {
			console.error('fetching machine info error : ', error);
			is_error = true;
		}
	};

	const check_active_status = async () => {
		try {
			let data: any = await check_ping_status();
			is_active = data?.code == 'success';
		} catch (error) {
			is_active = false;
			is_error = true;
			error_message = 'Machine agent not running or internet connectivity';
			console.error('Error: checking ping for status : ', error);
		}
	};

	onMount(() => {
		check_active_status();

		setTimeout(() => (is_loading = false), 3000);
		get_machine_data();
	});

	$: if (is_error) is_loading = false;

	setInterval(async () => {
		check_active_status();
	}, 10000);

	setInterval(() => {
		get_machine_data();
	}, 1200);

	const exitApp = () => {
		invoke('exit_app');
	};
</script>

<Layout>
	<div
		class="flex h-full w-full flex-col justify-items-start"
		style="height:-webkit-fill-available"
	>
		<div class="relative m-4 flex flex-col items-start justify-start">
			<div class="">
				<img
					class="rounded-lg"
					width="120"
					height="120"
					src={machine_icon == '' ? MechaCompute : machine_icon}
					alt="Machine Profile"
				/>
			</div>

			<div class="flex flex-row items-center gap-1 text-xl">
				<p>{$machineInfo.name}</p>

				{#if is_active}
					<Icons name="active_status_icon" width="32" height="32" />
				{:else}
					<Icons name="idle_status_icon" width="32" height="32" />
				{/if}
			</div>
		</div>

		<div
			class="my-2 flex flex-row justify-between border-y border-solid border-[#535353] p-4 text-base capitalize tracking-widest"
		>
			<div>MACHINE ID</div>
			<div>{$machineInfo.id}</div>
		</div>

		<div class="mx-2 text-base font-medium">
			You can unlink your machine from your Mecha account
		</div>
		<Dialog.Root bind:open={is_error}>
			<Dialog.Content class="w-[90%] bg-[#1D1D1D] border-[#292929] rounded-lg">
				<Dialog.Header>
					<Dialog.Title class="flex justify-start">
						<div class="flex flex-row items-center gap-2">
							<Icons name="attention" width="40" height="40" />
							<span class="text-left"> {error_message} </span>
						</div>
					</Dialog.Title>
				</Dialog.Header>
				<div class="justify-self-end">
					<Button
						class="hover:bg-[#525251] border-0 outline-none flex justify-end gap-1 bg-[#444444] border-[#1D1D1D] rounded-lg text-xl"
						type="button"
						on:click={() => {
							console.log('is_error:: ERROR ON CLICK', is_error);
							is_loading = false;
							is_error = false;
						}}
					>
						<div class="flex flex-row items-center gap-2">Close</div>
					</Button>
				</div>
				<!-- <Dialog.Footer class="border-t border-[#848484]"> -->
				<!-- <Dialog.Footer class="">
					<Button
						class="bg-[#1D1D1D] border-0 outline-none flex justify-end gap-1"
						type="button"
						on:click={() => {
							console.log('is_error:: ERROR ON CLICK', is_error);
							is_loading = false;
							is_error = false;
						}}
					>
						<div class="flex flex-row items-center gap-2">
							<Icons name="close_icon" width="20" height="20" />
							Close
						</div>
					</Button>
				</Dialog.Footer> -->
			</Dialog.Content>
		</Dialog.Root>

		<!--  border-y-[#848484] border-x-0 -->
		<Dialog.Root bind:open={is_loading}>
			<Dialog.Content class="w-[90%] bg-[#1D1D1D] border-[#292929] rounded-lg">
				<Dialog.Header>
					<Dialog.Title class="flex justify-start">
						<div class="text-xl flex flex-row items-center gap-2">
							<!-- <Icons name="info" width="40" height="40" /> -->
							Fetching machine information...
							<Icons name="spinner" width="30" height="30" />
						</div>
					</Dialog.Title>
				</Dialog.Header>
				<!-- <Dialog.Footer class="border-t border-[#848484]">
					<Button
						class="bg-[#1D1D1D] border-0 outline-none flex justify-end"
						type="button"
						on:click={() => {
							console.log('is_loading:: ERROR ON CLICK');
							is_loading = false;
						}}
					>
						<div class="flex flex-row items-center gap-2">
							<Icons name="close_icon" width="20" height="20" />
							Close
						</div>
					</Button>
				</Dialog.Footer> -->
			</Dialog.Content>
		</Dialog.Root>
	</div>
	<footer slot="footer" class="h-full w-full bg-[#05070A73] backdrop-blur-3xl backdrop-filter">
		<div
			class="border-silver-gray flex h-full w-full flex-row items-center justify-end border-t-2 px-4 py-3"
		>
			<button
				class="flex h-[60px] w-[60px] items-center justify-center rounded-lg p-2 text-[#FAFBFC]"
				on:click={exitApp}
			>
				<Icons name="exit_icon" width="60" height="60" />
			</button>
		</div>
	</footer>
</Layout>
